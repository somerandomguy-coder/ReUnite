package com.reunite.reunite_mobile

import android.annotation.SuppressLint
import android.bluetooth.*
import android.bluetooth.le.*
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.util.Log
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * The Bluetooth radio for the mesh (plan.md §4 step 2.1).
 *
 * Every device plays **both roles at once**, which is what makes a mesh out of BLE:
 *
 *  * **Peripheral** — advertises the service UUID so others can find it, and runs a GATT
 *    server with an RX characteristic peers write frames into and a TX characteristic it
 *    notifies frames out on.
 *  * **Central** — scans for that same service UUID, connects to whatever it finds,
 *    subscribes to TX and writes to RX.
 *
 * Being symmetric means it does not matter who discovered whom: once two phones are
 * connected, frames flow both ways over whichever link exists.
 *
 * The service and characteristic UUIDs match `crates/meshcore/src/transport/ble_linux.rs`,
 * so a Linux laptop running `meshnet --transport ble` is just another peer.
 *
 * Nothing here understands the mesh. It moves opaque byte frames; routing, encryption,
 * dedupe and every protocol decision stay in Rust.
 */
class BleMesh(
    private val context: Context,
    private val onFrame: (frame: ByteArray, device: String) -> Unit,
    private val onPeerLost: (device: String) -> Unit,
    private val onLog: (String) -> Unit,
    private val onState: (String) -> Unit = {},
    private val onRssi: (device: String, rssi: Int) -> Unit = { _, _ -> },
) {
    companion object {
        val SERVICE_UUID: UUID = UUID.fromString("a1b2c3d4-e5f6-7890-1234-56789abcdef0")
        val RX_CHAR_UUID: UUID = UUID.fromString("a1b2c3d4-e5f6-7890-1234-56789abcdef1")
        val TX_CHAR_UUID: UUID = UUID.fromString("a1b2c3d4-e5f6-7890-1234-56789abcdef2")
        val CCCD_UUID: UUID = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")
        private const val TAG = "ReUniteBle"
        /** Leaves headroom under the negotiated ATT MTU (MTU - 3 for the write header). */
        private const val REQUESTED_MTU = 512
    }

    private val manager = context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
    private val adapter: BluetoothAdapter? get() = manager.adapter
    private val main = Handler(Looper.getMainLooper())

    private var gattServer: BluetoothGattServer? = null
    private var txCharacteristic: BluetoothGattCharacteristic? = null
    private var advertiser: BluetoothLeAdvertiser? = null
    private var scanner: BluetoothLeScanner? = null

    /** Centrals that have subscribed to our TX characteristic. */
    private val subscribers = ConcurrentHashMap<String, BluetoothDevice>()
    /** Peripherals we connected out to, as a central. */
    private val clients = ConcurrentHashMap<String, BluetoothGatt>()
    private val clientRx = ConcurrentHashMap<String, BluetoothGattCharacteristic>()
    private val mtu = ConcurrentHashMap<String, Int>()
    private val reassemblers = ConcurrentHashMap<String, FrameCodec.Reassembler>()
    /** Devices a connection attempt is already in flight for. */
    private val connecting = ConcurrentHashMap<String, Long>()

    @Volatile private var running = false

    // ------------------------------------------------------------------- duty cycle

    /** Current scan mode, one of ScanSettings' constants. */
    private var scanMode = ScanSettings.SCAN_MODE_LOW_LATENCY
    /** Windowed scanning: listen for [scanWindowMs], then sleep until [scanPeriodMs]. */
    private var scanWindowMs: Long? = null
    private var scanPeriodMs: Long? = null
    private var scanning = false
    private val dutyRunnable = Runnable { cycleScan() }

    /**
     * Change how hard the radio listens (phase 2D).
     *
     * Scanning is the expensive half of being in a mesh - a receiver listening at full
     * tilt costs far more than a transmitter speaking occasionally - so this is where
     * backing off actually buys battery. Android exposes both knobs: the scan mode, and
     * (by stopping and restarting) a duty cycle.
     */
    @SuppressLint("MissingPermission")
    fun setCadence(scan: String, windowMs: Long?, periodMs: Long?) {
        val mode = when (scan) {
            "balanced" -> ScanSettings.SCAN_MODE_BALANCED
            "low_power" -> ScanSettings.SCAN_MODE_LOW_POWER
            else -> ScanSettings.SCAN_MODE_LOW_LATENCY
        }
        if (mode == scanMode && windowMs == scanWindowMs && periodMs == scanPeriodMs) return
        scanMode = mode
        scanWindowMs = windowMs
        scanPeriodMs = periodMs
        onLog("scan cadence -> $scan" + if (windowMs != null) " (${windowMs}ms every ${periodMs}ms)" else "")
        if (!running) return
        main.removeCallbacks(dutyRunnable)
        stopScanning()
        startScanning()
    }

    /** One step of the windowed scan: flip the scanner and schedule the next flip. */
    @SuppressLint("MissingPermission")
    private fun cycleScan() {
        if (!running) return
        val window = scanWindowMs
        val period = scanPeriodMs
        if (window == null || period == null) return
        if (scanning) {
            stopScanning()
            main.postDelayed(dutyRunnable, (period - window).coerceAtLeast(0))
        } else {
            startScanning()
        }
    }

    fun isSupported(): Boolean =
        adapter != null && context.packageManager.hasSystemFeature("android.hardware.bluetooth_le")

    fun isEnabled(): Boolean = adapter?.isEnabled == true

    /** The word Dart uses to decide what, if anything, to tell the user. */
    fun state(): String = when {
        adapter == null -> "unsupported"
        adapter?.isEnabled == true -> "on"
        else -> "off"
    }

    /**
     * The adapter can be switched off while the mesh is running, and the app has to say
     * so rather than looking as though the mesh simply went quiet. Mirrors the iOS
     * `centralManagerDidUpdateState` callback, which is push-only by design.
     */
    private val adapterWatcher = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == BluetoothAdapter.ACTION_STATE_CHANGED) {
                onState(state())
            }
        }
    }
    private var watching = false

    /** True once at least one peer can actually receive a frame. */
    fun connectedCount(): Int = (subscribers.keys + clientRx.keys).size

    // ------------------------------------------------------------------ lifecycle

    @SuppressLint("MissingPermission")
    fun start(): String? {
        if (running) return null
        val adapter = this.adapter ?: return "no Bluetooth adapter on this device"
        if (!adapter.isEnabled) return "Bluetooth is turned off"
        running = true
        return try {
            startGattServer()
            startAdvertising()
            startScanning()
            if (!watching) {
                context.registerReceiver(
                    adapterWatcher,
                    IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED),
                )
                watching = true
            }
            onState(state())
            onLog("BLE mesh started; advertising and scanning for $SERVICE_UUID")
            null
        } catch (e: SecurityException) {
            running = false
            "Bluetooth permission was refused: ${e.message}"
        } catch (e: Exception) {
            running = false
            "could not start BLE: ${e.message}"
        }
    }

    @SuppressLint("MissingPermission")
    fun stop() {
        if (!running) return
        running = false
        main.removeCallbacks(dutyRunnable)
        try {
            advertiser?.stopAdvertising(advertiseCallback)
            stopScanning()
            clients.values.forEach { runCatching { it.close() } }
            gattServer?.close()
        } catch (e: SecurityException) {
            Log.w(TAG, "permission lost during stop: ${e.message}")
        }
        if (watching) {
            runCatching { context.unregisterReceiver(adapterWatcher) }
            watching = false
        }
        clients.clear(); clientRx.clear(); subscribers.clear()
        reassemblers.clear(); mtu.clear(); connecting.clear()
        gattServer = null
        onLog("BLE mesh stopped")
    }

    // ------------------------------------------------------------- peripheral role

    @SuppressLint("MissingPermission")
    private fun startGattServer() {
        val server = manager.openGattServer(context, serverCallback)
            ?: throw IllegalStateException("could not open a GATT server")
        val service = BluetoothGattService(SERVICE_UUID, BluetoothGattService.SERVICE_TYPE_PRIMARY)

        // Peers write frames into RX. Write-without-response keeps throughput up; the
        // mesh tolerates loss because every packet class is re-sent or retried.
        val rx = BluetoothGattCharacteristic(
            RX_CHAR_UUID,
            BluetoothGattCharacteristic.PROPERTY_WRITE or
                BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE,
            BluetoothGattCharacteristic.PERMISSION_WRITE,
        )
        val tx = BluetoothGattCharacteristic(
            TX_CHAR_UUID,
            BluetoothGattCharacteristic.PROPERTY_NOTIFY,
            BluetoothGattCharacteristic.PERMISSION_READ,
        )
        tx.addDescriptor(
            BluetoothGattDescriptor(
                CCCD_UUID,
                BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE,
            )
        )
        service.addCharacteristic(rx)
        service.addCharacteristic(tx)
        server.addService(service)
        gattServer = server
        txCharacteristic = tx
    }

    private val serverCallback = object : BluetoothGattServerCallback() {
        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            val id = device.address
            if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                subscribers.remove(id)
                reassemblers.remove(id)
                if (!clientRx.containsKey(id)) onPeerLost(id)
            }
        }

        @SuppressLint("MissingPermission")
        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray,
        ) {
            if (characteristic.uuid == RX_CHAR_UUID) {
                ingest(device.address, value)
            }
            if (responseNeeded) {
                runCatching {
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                }
            }
        }

        @SuppressLint("MissingPermission")
        override fun onDescriptorWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            descriptor: BluetoothGattDescriptor,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray,
        ) {
            if (descriptor.uuid == CCCD_UUID) {
                // A central subscribing to TX is what makes it reachable by notification.
                subscribers[device.address] = device
                onLog("peer ${device.address} subscribed")
            }
            if (responseNeeded) {
                runCatching {
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                }
            }
        }

        override fun onMtuChanged(device: BluetoothDevice, newMtu: Int) {
            mtu[device.address] = newMtu
        }
    }

    @SuppressLint("MissingPermission")
    private fun startAdvertising() {
        val adv = adapter?.bluetoothLeAdvertiser
            ?: throw IllegalStateException(
                "this device cannot advertise over BLE (no peripheral role support)"
            )
        advertiser = adv
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY)
            .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_HIGH)
            .setConnectable(true)
            .setTimeout(0)
            .build()
        // The service UUID alone fills most of the 31-byte advertisement, so the name
        // goes in the scan response rather than crowding it out.
        val data = AdvertiseData.Builder()
            .addServiceUuid(ParcelUuid(SERVICE_UUID))
            .setIncludeDeviceName(false)
            .build()
        val scanResponse = AdvertiseData.Builder().setIncludeDeviceName(true).build()
        adv.startAdvertising(settings, data, scanResponse, advertiseCallback)
    }

    private val advertiseCallback = object : AdvertiseCallback() {
        override fun onStartFailure(errorCode: Int) {
            onLog("advertising failed with code $errorCode")
        }
        override fun onStartSuccess(settingsInEffect: AdvertiseSettings) {
            onLog("advertising as a mesh node")
        }
    }

    // ---------------------------------------------------------------- central role

    @SuppressLint("MissingPermission")
    private fun startScanning() {
        val sc = adapter?.bluetoothLeScanner ?: throw IllegalStateException("no BLE scanner")
        scanner = sc
        val filters = listOf(
            ScanFilter.Builder().setServiceUuid(ParcelUuid(SERVICE_UUID)).build()
        )
        val settings = ScanSettings.Builder().setScanMode(scanMode).build()
        sc.startScan(filters, settings, scanCallback)
        scanning = true
        // If a window is set, schedule the sleep half of the cycle.
        scanWindowMs?.let { main.postDelayed(dutyRunnable, it) }
    }

    @SuppressLint("MissingPermission")
    private fun stopScanning() {
        if (!scanning) return
        runCatching { scanner?.stopScan(scanCallback) }
        scanning = false
    }

    private val scanCallback = object : ScanCallback() {
        @SuppressLint("MissingPermission")
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            // Every advertisement carries a fresh reading, including from devices we
            // never connect to. It is the only proximity signal BLE gives us.
            onRssi(result.device.address, result.rssi)
            connectTo(result.device, result.rssi)
        }
        override fun onScanFailed(errorCode: Int) {
            onLog("scan failed with code $errorCode")
        }
    }

    @SuppressLint("MissingPermission")
    private fun connectTo(device: BluetoothDevice, rssi: Int) {
        val id = device.address
        if (!running || clients.containsKey(id)) return
        val now = System.currentTimeMillis()
        val last = connecting[id]
        // Scan results repeat constantly; without this throttle every result would start
        // another connection attempt and the stack would collapse under them.
        if (last != null && now - last < 10_000) return
        connecting[id] = now
        main.post {
            runCatching {
                val gatt = device.connectGatt(context, false, clientCallback, BluetoothDevice.TRANSPORT_LE)
                clients[id] = gatt
                onLog("connecting to $id (rssi $rssi)")
            }.onFailure { onLog("connect to $id failed: ${it.message}") }
        }
    }

    private val clientCallback = object : BluetoothGattCallback() {
        @SuppressLint("MissingPermission")
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            val id = gatt.device.address
            when (newState) {
                BluetoothProfile.STATE_CONNECTED -> {
                    runCatching { gatt.requestMtu(REQUESTED_MTU) }
                }
                BluetoothProfile.STATE_DISCONNECTED -> {
                    clients.remove(id)?.also { runCatching { it.close() } }
                    clientRx.remove(id)
                    reassemblers.remove(id)
                    connecting.remove(id)
                    if (!subscribers.containsKey(id)) onPeerLost(id)
                }
            }
        }

        @SuppressLint("MissingPermission")
        override fun onMtuChanged(gatt: BluetoothGatt, newMtu: Int, status: Int) {
            mtu[gatt.device.address] = newMtu
            runCatching { gatt.discoverServices() }
        }

        @SuppressLint("MissingPermission")
        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            val id = gatt.device.address
            val service = gatt.getService(SERVICE_UUID) ?: run {
                onLog("$id has no mesh service; disconnecting")
                runCatching { gatt.disconnect() }
                return
            }
            service.getCharacteristic(RX_CHAR_UUID)?.let { clientRx[id] = it }
            service.getCharacteristic(TX_CHAR_UUID)?.let { tx ->
                runCatching {
                    gatt.setCharacteristicNotification(tx, true)
                    tx.getDescriptor(CCCD_UUID)?.let { cccd ->
                        writeDescriptorCompat(gatt, cccd, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
                    }
                }
            }
            onLog("$id is a mesh peer")
        }

        override fun onCharacteristicChanged(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
            value: ByteArray,
        ) {
            if (characteristic.uuid == TX_CHAR_UUID) ingest(gatt.device.address, value)
        }

        @Deprecated("Pre-API-33 callback; Android still calls it on older devices.")
        @Suppress("DEPRECATION")
        override fun onCharacteristicChanged(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
        ) {
            if (characteristic.uuid == TX_CHAR_UUID) {
                characteristic.value?.let { ingest(gatt.device.address, it) }
            }
        }
    }

    // -------------------------------------------------------------------- transfer

    private fun ingest(device: String, bytes: ByteArray) {
        val reassembler = reassemblers.getOrPut(device) { FrameCodec.Reassembler() }
        for (frame in reassembler.add(bytes)) {
            onFrame(frame, device)
        }
    }

    /**
     * Put a frame on the air. [target] names one device, or null to reach every peer.
     *
     * Both roles are used: subscribed centrals get a notification from our GATT server,
     * peripherals we connected out to get a write. A device reachable both ways is sent
     * to once — meshcore would dedupe a repeat, but there is no reason to pay for it.
     */
    @SuppressLint("MissingPermission")
    fun send(frame: ByteArray, target: String?): Int {
        if (!running) return 0
        val payload = FrameCodec.encode(frame)
        var reached = 0
        val done = HashSet<String>()

        for ((id, device) in subscribers) {
            if (target != null && id != target) continue
            if (!done.add(id)) continue
            val tx = txCharacteristic ?: break
            val chunks = FrameCodec.chunk(payload, (mtu[id] ?: 23) - 3)
            var ok = true
            for (chunk in chunks) {
                ok = ok && notifyCompat(device, tx, chunk)
            }
            if (ok) reached++
        }

        for ((id, characteristic) in clientRx) {
            if (target != null && id != target) continue
            if (!done.add(id)) continue
            val gatt = clients[id] ?: continue
            val chunks = FrameCodec.chunk(payload, (mtu[id] ?: 23) - 3)
            var ok = true
            for (chunk in chunks) {
                ok = ok && writeCompat(gatt, characteristic, chunk)
            }
            if (ok) reached++
        }
        return reached
    }

    // Android 13 replaced the value-carrying GATT calls; both forms are needed because
    // the app supports older devices, which are exactly the ones people still own.
    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    private fun notifyCompat(
        device: BluetoothDevice,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray,
    ): Boolean = runCatching {
        val server = gattServer ?: return false
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            server.notifyCharacteristicChanged(device, characteristic, false, value) ==
                BluetoothStatusCodes.SUCCESS
        } else {
            characteristic.value = value
            server.notifyCharacteristicChanged(device, characteristic, false)
        }
    }.getOrDefault(false)

    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    private fun writeCompat(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray,
    ): Boolean = runCatching {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            gatt.writeCharacteristic(
                characteristic, value,
                BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE,
            ) == BluetoothStatusCodes.SUCCESS
        } else {
            characteristic.value = value
            characteristic.writeType = BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE
            gatt.writeCharacteristic(characteristic)
        }
    }.getOrDefault(false)

    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    private fun writeDescriptorCompat(
        gatt: BluetoothGatt,
        descriptor: BluetoothGattDescriptor,
        value: ByteArray,
    ) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            gatt.writeDescriptor(descriptor, value)
        } else {
            descriptor.value = value
            gatt.writeDescriptor(descriptor)
        }
    }
}
