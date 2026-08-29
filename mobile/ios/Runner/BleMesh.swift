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
    private let onState: (String) -> Void
    private let onRssi: (String, Int) -> Void

    private var running = false

    /// Windowed scanning: listen for `scanWindow`, then sleep until `scanPeriod`.
    ///
    /// CoreBluetooth has **no scan-mode knob** - it decides for itself how aggressively
    /// to listen, and there is no iOS equivalent of Android's `ScanSettings`. Nor can an
    /// app set its advertising interval. So the only duty cycle available here is
    /// stopping and restarting the scan, which is what this does. Saying so plainly
    /// matters: the same ladder buys real battery on Android and much less on iOS, and a
    /// measurement that assumes otherwise will be confusing.
    private var scanWindow: TimeInterval?
    private var scanPeriod: TimeInterval?
    private var dutyTimer: Timer?
    private var scanning = false

    /// Connection attempts already in flight, and when they started.
    ///
    /// The scan runs with `allowDuplicates`, so a peripheral advertises into
    /// `didDiscover` several times a second. Without this throttle every one of those
    /// starts another `connect()` for a peripheral whose connection has not finished,
    /// which is how CoreBluetooth ends up wedged. Android's `BleMesh.kt` has had the
    /// same 10-second guard since it was written; this side did not.
    private var connecting: [String: Date] = [:]
    private static let connectRetryAfter: TimeInterval = 10

    /// Chunks still to write out, per peripheral, and to notify to subscribed centrals.
    ///
    /// CoreBluetooth silently discards writes and notifications queued past its limit.
    /// The previous code pushed every chunk in a loop and treated a `false` return as
    /// "that one failed" - so a frame split across five writes arrived truncated, the
    /// length-prefixed reassembler waited forever for bytes that were never coming, and
    /// the link looked connected while passing nothing.
    private var writeQueue: [String: [Data]] = [:]
    private var notifyQueue: [Data] = []

    init(
        onFrame: @escaping (String, String) -> Void,
        onPeerLost: @escaping (String) -> Void,
        onLog: @escaping (String) -> Void,
        onState: @escaping (String) -> Void = { _ in },
        onRssi: @escaping (String, Int) -> Void = { _, _ in }
    ) {
        self.onFrame = onFrame
        self.onPeerLost = onPeerLost
        self.onLog = onLog
        self.onState = onState
        self.onRssi = onRssi
        super.init()
    }

    /// CoreBluetooth's state as a word Dart can act on.
    static func describe(_ state: CBManagerState) -> String {
        switch state {
        case .poweredOn: return "on"
        case .poweredOff: return "off"
        case .unauthorized: return "unauthorized"
        case .unsupported: return "unsupported"
        case .resetting: return "resetting"
        default: return "unknown"
        }
    }

    /// The last state CoreBluetooth reported, or `.unknown` before it has.
    var currentState: CBManagerState { central?.state ?? .unknown }

    var isSupported: Bool { true }

    /// Whether the radio is powered on **as far as we currently know**.
    ///
    /// CoreBluetooth cannot answer this synchronously before a manager exists, and any
    /// caller that treats a pre-`start()` answer as authoritative gets `false` every
    /// time. That is exactly the bug this class shipped with: Dart asked, got `false`,
    /// and refused to start the radio - telling the user to switch on Bluetooth that was
    /// already on. Power state is reported through `onState` instead, when it is known.
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
        dutyTimer?.invalidate()
        dutyTimer = nil
        scanning = false
        central?.stopScan()
        peripheral?.stopAdvertising()
        for (_, p) in peers { central?.cancelPeripheralConnection(p) }
        peers.removeAll(); peerRx.removeAll(); buffers.removeAll()
        subscribers.removeAll(); discovered.removeAll()
        connecting.removeAll(); writeQueue.removeAll(); notifyQueue.removeAll()
        peripheral?.removeAllServices()
        central = nil
        peripheral = nil
        onLog("BLE mesh stopped")
    }

    // MARK: - duty cycle

    /// Change how hard the radio listens (phase 2D). `scan` is accepted for parity with
    /// Android and recorded for the log, but only the window is actionable here.
    func setCadence(scan: String, windowMs: Int?, periodMs: Int?) {
        let window = windowMs.map { TimeInterval($0) / 1000 }
        let period = periodMs.map { TimeInterval($0) / 1000 }
        if window == scanWindow && period == scanPeriod { return }
        scanWindow = window
        scanPeriod = period
        onLog("scan cadence -> \(scan)" + (window.map { " (\($0)s every \(period ?? 0)s)" } ?? ""))
        guard running else { return }
        dutyTimer?.invalidate()
        dutyTimer = nil
        startScanning()
    }

    private func startScanning() {
        guard running, let manager = central, manager.state == .poweredOn else { return }
        manager.scanForPeripherals(
            withServices: [Self.serviceUUID],
            // Duplicates carry fresh RSSI, which is the only proximity signal BLE gives
            // us, and the mesh wants it.
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: true]
        )
        scanning = true
        guard let window = scanWindow else { return }
        dutyTimer = Timer.scheduledTimer(withTimeInterval: window, repeats: false) { [weak self] _ in
            self?.pauseScanning()
        }
    }

    private func pauseScanning() {
        guard running, let window = scanWindow, let period = scanPeriod else { return }
        central?.stopScan()
        scanning = false
        dutyTimer = Timer.scheduledTimer(
            withTimeInterval: max(period - window, 0),
            repeats: false
        ) { [weak self] _ in
            self?.startScanning()
        }
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
    /// Returns how many peers it was queued for.
    ///
    /// "Queued", not "delivered": CoreBluetooth takes chunks only as fast as its transmit
    /// queue drains, so everything here is enqueued and pumped from the two
    /// `...IsReady...` delegate callbacks. Claiming delivery at this point is what made
    /// the old code report success for frames it had thrown away.
    @discardableResult
    func send(frameHex: String, to target: String?) -> Int {
        guard running, let frame = Data(hex: frameHex) else { return 0 }
        let payload = encode(frame)
        var reached = 0
        var done = Set<String>()

        // As peripheral: notify every subscribed central.
        if txCharacteristic != nil, !subscribers.isEmpty {
            let targets = subscribers.filter { target == nil || $0.identifier.uuidString == target }
            if !targets.isEmpty {
                let mtu = targets.map { $0.maximumUpdateValueLength }.min() ?? 20
                notifyQueue.append(contentsOf: chunk(payload, size: mtu))
                pumpNotifications()
                for c in targets where done.insert(c.identifier.uuidString).inserted { reached += 1 }
            }
        }

        // As central: write to every peripheral we connected out to.
        for (id, _) in peerRx {
            if let target = target, id != target { continue }
            if !done.insert(id).inserted { continue }
            guard let p = peers[id] else { continue }
            let mtu = p.maximumWriteValueLength(for: .withoutResponse)
            writeQueue[id, default: []].append(contentsOf: chunk(payload, size: mtu))
            pumpWrites(to: id)
            reached += 1
        }
        return reached
    }

    /// Drain queued notifications while the peripheral manager will take them.
    ///
    /// `updateValue` returns false when its queue is full; the remaining chunks stay
    /// queued and `peripheralManagerIsReady(toUpdateSubscribers:)` calls back here.
    private func pumpNotifications() {
        guard let manager = peripheral, let tx = txCharacteristic else { return }
        while let next = notifyQueue.first {
            if manager.updateValue(next, for: tx, onSubscribedCentrals: nil) {
                notifyQueue.removeFirst()
            } else {
                return   // queue full; resumed from peripheralManagerIsReady
            }
        }
    }

    /// Drain queued writes for one peripheral while it will accept them.
    private func pumpWrites(to id: String) {
        guard let p = peers[id], let characteristic = peerRx[id] else { return }
        while let next = writeQueue[id]?.first {
            guard p.canSendWriteWithoutResponse else {
                return   // resumed from peripheralIsReady(toSendWriteWithoutResponse:)
            }
            p.writeValue(next, for: characteristic, type: .withoutResponse)
            writeQueue[id]?.removeFirst()
        }
        if writeQueue[id]?.isEmpty == true { writeQueue[id] = nil }
    }
}

// MARK: - peripheral role

extension BleMesh: CBPeripheralManagerDelegate {
    func peripheralManagerDidUpdateState(_ manager: CBPeripheralManager) {
        onState(Self.describe(manager.state))
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
        // Every request needs its own response. Answering only the first leaves a central
        // waiting on a `.withResponse` write until it times out, which stalls that link.
        for request in requests {
            manager.respond(to: request, withResult: .success)
        }
    }

    /// The transmit queue drained; keep going.
    func peripheralManagerIsReady(toUpdateSubscribers manager: CBPeripheralManager) {
        pumpNotifications()
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
        // Reported whether or not we are running: this is the only authoritative answer
        // to "is Bluetooth on", and Dart needs it to decide what to tell the user.
        onState(Self.describe(manager.state))
        guard running else { return }
        switch manager.state {
        case .poweredOn:
            startScanning()
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
        // Signal strength is the only proximity measure BLE gives us, and it arrives
        // here on every advertisement - including from peers we never connect to.
        onRssi(id, RSSI.intValue)

        guard peers[id] == nil else { return }
        // Scan results repeat several times a second. Without this throttle every repeat
        // starts another connection attempt for a peripheral whose connection is still in
        // flight, and the stack collapses under them. Mirrors Android's `connectTo`.
        if let started = connecting[id], Date().timeIntervalSince(started) < Self.connectRetryAfter {
            return
        }
        connecting[id] = Date()

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
        connecting[id] = nil
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
        connecting[id] = nil
        writeQueue[id] = nil
        if !subscribers.contains(where: { $0.identifier.uuidString == id }) { onPeerLost(id) }
    }

    func centralManager(
        _ manager: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        let id = peripheral.identifier.uuidString
        discovered[id] = nil
        // Clear the throttle so the next advertisement retries immediately rather than
        // waiting out a window that was meant for a connection still in progress.
        connecting[id] = nil
        onLog("connect to \(id) failed: \(error?.localizedDescription ?? "unknown")")
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
        // Anything queued while the characteristics were still being discovered.
        pumpWrites(to: id)
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard characteristic.uuid == Self.txUUID, let value = characteristic.value else { return }
        ingest(value, from: peripheral.identifier.uuidString)
    }

    /// The peripheral will take more writes; continue where `pumpWrites` stopped.
    func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        pumpWrites(to: peripheral.identifier.uuidString)
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
