import CoreBluetooth
import Foundation

/// The Bluetooth radio for the mesh on macOS (plan.md §4 step 2.1).
///
/// Mirrors `BleMesh.swift` on iOS - same UUIDs, same length-prefixed framing - so a Mac
/// is just another peer to an iPhone, an Android phone, or a Linux laptop running
/// `meshnet --transport ble`.
///
/// Unlike iOS, this plays **central role only**: macOS does not expose BLE
/// peripheral-role advertising portably from userspace, so a Mac can connect out to a
/// phone that's advertising, but cannot advertise itself or be discovered by another
/// Mac. See docs/ARCHITECTURE.md. If that ever changes, a peripheral-role extension can
/// be added here the same way iOS splits its central and peripheral roles into separate
/// `extension BleMesh: CB...Delegate` blocks - nothing below assumes central-only.
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
    var connectedCount: Int { peerRx.count }

    func start() -> String? {
        if running { return nil }
        running = true
        central = CBCentralManager(delegate: self, queue: .main)
        onLog("BLE central starting")
        return nil
    }

    func stop() {
        guard running else { return }
        running = false
        central?.stopScan()
        for (_, p) in peers { central?.cancelPeripheralConnection(p) }
        peers.removeAll(); peerRx.removeAll(); buffers.removeAll(); discovered.removeAll()
        central = nil
        onLog("BLE central stopped")
    }

    // MARK: - framing

    /// Length-prefixed framing, identical to iOS/Android: a 4-byte little-endian length
    /// then the frame, split across as many writes as the MTU needs.
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

        for (id, characteristic) in peerRx {
            if let target = target, id != target { continue }
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
        onPeerLost(id)
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
