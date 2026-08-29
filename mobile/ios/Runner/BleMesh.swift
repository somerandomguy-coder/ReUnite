import CoreBluetooth
import Foundation

/// The Bluetooth radio for the mesh on iOS (plan.md §4 step 2.1).
///
/// Mirrors `BleMesh.kt` on Android, including the UUIDs, so an iPhone, an Android phone
/// and a Linux laptop running `meshnet --transport ble` are all just peers to each other.
///
/// Every device plays both roles at once, which is what makes a mesh out of BLE:
///  - **Peripheral**: advertises the service and hosts RX (peers write frames in) and
///    TX (we notify frames out).
///  - **Central**: scans for the same service, connects, subscribes to TX, writes to RX.
///
/// Nothing here understands the mesh. It moves opaque frames; routing, encryption and
/// every protocol decision stay in Rust.
final class BleMesh: NSObject {
    static let serviceUUID = CBUUID(string: "a1b2c3d4-e5f6-7890-1234-56789abcdef0")
    static let rxUUID = CBUUID(string: "a1b2c3d4-e5f6-7890-1234-56789abcdef1")
    static let txUUID = CBUUID(string: "a1b2c3d4-e5f6-7890-1234-56789abcdef2")

    /// meshcore's MAX_FRAME_BYTES. A corrupt length must not make us allocate without bound.
    private static let maxFrameBytes = 8 * 1024

    private var central: CBCentralManager?
    private var peripheral: CBPeripheralManager?

    private var txCharacteristic: CBMutableCharacteristic?
    private var subscribers: [CBCentral] = []

    /// Peripherals we connected out to, keyed by the identifier we report to Dart.
    private var peers: [String: CBPeripheral] = [:]
    private var peerRx: [String: CBCharacteristic] = [:]
    private var buffers: [String: Data] = [:]
    private var discovered: [String: CBPeripheral] = [:]

    private let onFrame: (String, String) -> Void   // (frameHex, deviceId)
    private let onPeerLost: (String) -> Void
    private let onLog: (String) -> Void

    private var running = false

    init(
        onFrame: @escaping (String, String) -> Void,
        onPeerLost: @escaping (String) -> Void,
        onLog: @escaping (String) -> Void
    ) {
        self.onFrame = onFrame
        self.onPeerLost = onPeerLost
        self.onLog = onLog
        super.init()
    }

    var isSupported: Bool { true }
    var isEnabled: Bool { central?.state == .poweredOn }
    var connectedCount: Int { Set(peerRx.keys).union(subscribers.map { $0.identifier.uuidString }).count }

    func start() -> String? {
        if running { return nil }
        running = true
        // Both managers are created here; CoreBluetooth calls back on state once ready
        // and the actual advertising and scanning begin there.
        central = CBCentralManager(delegate: self, queue: .main)
        peripheral = CBPeripheralManager(delegate: self, queue: .main)
        onLog("BLE mesh starting")
        return nil
    }

    func stop() {
        guard running else { return }
        running = false
        central?.stopScan()
        peripheral?.stopAdvertising()
        for (_, p) in peers { central?.cancelPeripheralConnection(p) }
        peers.removeAll(); peerRx.removeAll(); buffers.removeAll()
        subscribers.removeAll(); discovered.removeAll()
        peripheral?.removeAllServices()
        central = nil
        peripheral = nil
        onLog("BLE mesh stopped")
    }

    // MARK: - framing

    /// Length-prefixed framing, identical to Android's `FrameCodec`: a 4-byte
    /// little-endian length then the frame, split across as many writes as the MTU needs.
    private func encode(_ frame: Data) -> Data {
        var out = Data(capacity: frame.count + 4)
        let n = UInt32(frame.count).littleEndian
        withUnsafeBytes(of: n) { out.append(contentsOf: $0) }
        out.append(frame)
        return out
    }

    private func chunk(_ payload: Data, size: Int) -> [Data] {
        let limit = max(size, 20)
        var chunks: [Data] = []
        var offset = 0
        while offset < payload.count {
            let end = min(offset + limit, payload.count)
            chunks.append(payload.subdata(in: offset..<end))
            offset = end
        }
        return chunks
    }

    private func ingest(_ bytes: Data, from device: String) {
        var buffer = (buffers[device] ?? Data()) + bytes
        while buffer.count >= 4 {
            let len = Int(buffer.prefix(4).withUnsafeBytes { $0.load(as: UInt32.self).littleEndian })
            if len <= 0 || len > Self.maxFrameBytes {
                // Desynchronised or hostile: drop this device's buffer rather than
                // trying to resync into arbitrary bytes.
                buffer = Data()
                break
            }
            if buffer.count - 4 < len { break }
            let frame = buffer.subdata(in: 4..<(4 + len))
            buffer = buffer.subdata(in: (4 + len)..<buffer.count)
            onFrame(frame.map { String(format: "%02x", $0) }.joined(), device)
        }
        buffers[device] = buffer
    }

    // MARK: - sending

    /// Put a frame on the air. `target` names one device, or nil to reach every peer.
    /// Returns how many peers it went to.
    @discardableResult
    func send(frameHex: String, to target: String?) -> Int {
        guard running, let frame = Data(hex: frameHex) else { return 0 }
        let payload = encode(frame)
        var reached = 0
        var done = Set<String>()

        // As peripheral: notify every subscribed central.
        if let tx = txCharacteristic, let manager = peripheral, !subscribers.isEmpty {
            let targets = subscribers.filter { target == nil || $0.identifier.uuidString == target }
            if !targets.isEmpty {
                let mtu = targets.map { $0.maximumUpdateValueLength }.min() ?? 20
                var ok = true
                for piece in chunk(payload, size: mtu) {
                    ok = manager.updateValue(piece, for: tx, onSubscribedCentrals: targets) && ok
                }
                if ok {
                    for c in targets where done.insert(c.identifier.uuidString).inserted { reached += 1 }
                }
            }
        }

        // As central: write to every peripheral we connected out to.
        for (id, characteristic) in peerRx {
            if let target = target, id != target { continue }
            if !done.insert(id).inserted { continue }
            guard let p = peers[id] else { continue }
            let mtu = p.maximumWriteValueLength(for: .withoutResponse)
            for piece in chunk(payload, size: mtu) {
                p.writeValue(piece, for: characteristic, type: .withoutResponse)
            }
            reached += 1
        }
        return reached
    }
}

// MARK: - peripheral role

extension BleMesh: CBPeripheralManagerDelegate {
    func peripheralManagerDidUpdateState(_ manager: CBPeripheralManager) {
        guard manager.state == .poweredOn, running else { return }
        let rx = CBMutableCharacteristic(
            type: Self.rxUUID,
            properties: [.write, .writeWithoutResponse],
            value: nil,
            permissions: [.writeable]
        )
        let tx = CBMutableCharacteristic(
            type: Self.txUUID,
            properties: [.notify],
            value: nil,
            permissions: [.readable]
        )
        let service = CBMutableService(type: Self.serviceUUID, primary: true)
        service.characteristics = [rx, tx]
        txCharacteristic = tx
        manager.removeAllServices()
        manager.add(service)
        manager.startAdvertising([CBAdvertisementDataServiceUUIDsKey: [Self.serviceUUID]])
        onLog("advertising as a mesh node")
    }

    func peripheralManager(_ manager: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        for request in requests where request.characteristic.uuid == Self.rxUUID {
            if let value = request.value {
                ingest(value, from: request.central.identifier.uuidString)
            }
        }
        if let first = requests.first {
            manager.respond(to: first, withResult: .success)
        }
    }

    func peripheralManager(
        _ manager: CBPeripheralManager,
        central: CBCentral,
        didSubscribeTo characteristic: CBCharacteristic
    ) {
        if !subscribers.contains(where: { $0.identifier == central.identifier }) {
            subscribers.append(central)
        }
        onLog("peer \(central.identifier.uuidString) subscribed")
    }

    func peripheralManager(
        _ manager: CBPeripheralManager,
        central: CBCentral,
        didUnsubscribeFrom characteristic: CBCharacteristic
    ) {
        subscribers.removeAll { $0.identifier == central.identifier }
        let id = central.identifier.uuidString
        buffers[id] = nil
        if peerRx[id] == nil { onPeerLost(id) }
    }
}

// MARK: - central role

extension BleMesh: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ manager: CBCentralManager) {
        guard running else { return }
        switch manager.state {
        case .poweredOn:
            manager.scanForPeripherals(
                withServices: [Self.serviceUUID],
                // Duplicates carry fresh RSSI, which is the only proximity signal BLE
                // gives us, and the mesh wants it.
                options: [CBCentralManagerScanOptionAllowDuplicatesKey: true]
            )
            onLog("scanning for mesh peers")
        case .poweredOff:
            onLog("Bluetooth is turned off")
        case .unauthorized:
            onLog("Bluetooth permission was refused")
        default:
            break
        }
    }

    func centralManager(
        _ manager: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        let id = peripheral.identifier.uuidString
        guard peers[id] == nil else { return }
        // Hold a strong reference: CoreBluetooth drops peripherals it does not own and
        // the connection silently never completes.
        discovered[id] = peripheral
        peripheral.delegate = self
        manager.connect(peripheral, options: nil)
        onLog("connecting to \(id) (rssi \(RSSI))")
    }

    func centralManager(_ manager: CBCentralManager, didConnect peripheral: CBPeripheral) {
        let id = peripheral.identifier.uuidString
        peers[id] = peripheral
        discovered[id] = nil
        peripheral.discoverServices([Self.serviceUUID])
    }

    func centralManager(
        _ manager: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        let id = peripheral.identifier.uuidString
        peers[id] = nil
        peerRx[id] = nil
        buffers[id] = nil
        if !subscribers.contains(where: { $0.identifier.uuidString == id }) { onPeerLost(id) }
    }

    func centralManager(
        _ manager: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        discovered[peripheral.identifier.uuidString] = nil
    }
}

extension BleMesh: CBPeripheralDelegate {
    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let service = peripheral.services?.first(where: { $0.uuid == Self.serviceUUID })
        else {
            central?.cancelPeripheralConnection(peripheral)
            return
        }
        peripheral.discoverCharacteristics([Self.rxUUID, Self.txUUID], for: service)
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        let id = peripheral.identifier.uuidString
        for characteristic in service.characteristics ?? [] {
            if characteristic.uuid == Self.rxUUID {
                peerRx[id] = characteristic
            } else if characteristic.uuid == Self.txUUID {
                peripheral.setNotifyValue(true, for: characteristic)
            }
        }
        onLog("\(id) is a mesh peer")
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard characteristic.uuid == Self.txUUID, let value = characteristic.value else { return }
        ingest(value, from: peripheral.identifier.uuidString)
    }
}

private extension Data {
    /// Parse the hex the Rust core and Dart exchange frames as.
    init?(hex: String) {
        guard hex.count % 2 == 0 else { return nil }
        var data = Data(capacity: hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            data.append(byte)
            index = next
        }
        self = data
    }
}
