package com.reunite.reunite_mobile

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel

/**
 * Wires the two radios to Dart.
 *
 *  * Wi-Fi: hold a MulticastLock, or Android silently discards mesh beacons and the app
 *    looks broken for reasons that have nothing to do with the mesh.
 *  * Bluetooth: expose [BleMesh] over a MethodChannel for control and sending, and an
 *    EventChannel for frames arriving off the air.
 *
 * Frames cross as hex strings. Dart hands them straight to the Rust core without looking
 * inside — every protocol decision stays in `meshcore`.
 */
class MainActivity : FlutterActivity() {
    companion object {
        private const val METHOD_CHANNEL = "reunite/ble"
        private const val EVENT_CHANNEL = "reunite/ble/events"
        private const val TAG = "ReUnite"
    }

    private var multicastLock: WifiManager.MulticastLock? = null
    private var ble: BleMesh? = null
    private var events: EventChannel.EventSink? = null
    private val main = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        acquireMulticastLock()
    }

    private fun acquireMulticastLock() {
        try {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            multicastLock = wifi.createMulticastLock("reunite-mesh").apply {
                setReferenceCounted(true)
                acquire()
            }
            Log.i(TAG, "multicast lock acquired - Wi-Fi mesh discovery can receive")
        } catch (e: Exception) {
            Log.w(TAG, "could not acquire multicast lock: ${e.message}")
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        EventChannel(flutterEngine.dartExecutor.binaryMessenger, EVENT_CHANNEL)
            .setStreamHandler(object : EventChannel.StreamHandler {
                override fun onListen(arguments: Any?, sink: EventChannel.EventSink?) {
                    events = sink
                }
                override fun onCancel(arguments: Any?) {
                    events = null
                }
            })

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, METHOD_CHANNEL)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "isSupported" -> result.success(radio().isSupported())
                    "isEnabled" -> result.success(radio().isEnabled())
                    "state" -> result.success(radio().state())
                    "start" -> {
                        val error = radio().start()
                        if (error == null) result.success(true)
                        else result.error("BLE_START_FAILED", error, null)
                    }
                    "stop" -> { ble?.stop(); result.success(true) }
                    "connectedCount" -> result.success(ble?.connectedCount() ?: 0)
                    "setCadence" -> {
                        radio().setCadence(
                            call.argument<String>("scan") ?: "low_latency",
                            (call.argument<Number>("windowMs"))?.toLong(),
                            (call.argument<Number>("periodMs"))?.toLong(),
                        )
                        result.success(true)
                    }
                    "send" -> {
                        val hex = call.argument<String>("frame")
                        val target = call.argument<String>("to")
                        if (hex == null) {
                            result.error("BAD_ARGS", "missing 'frame'", null)
                        } else {
                            result.success(radio().send(hex.hexToBytes(), target))
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private fun radio(): BleMesh {
        ble?.let { return it }
        val created = BleMesh(
            context = applicationContext,
            onFrame = { frame, device ->
                // Platform channels are main-thread only; BLE callbacks are not on it.
                main.post {
                    events?.success(
                        mapOf("type" to "frame", "frame" to frame.toHex(), "from" to device)
                    )
                }
            },
            onPeerLost = { device ->
                main.post { events?.success(mapOf("type" to "peer_lost", "device" to device)) }
            },
            onLog = { message ->
                Log.i(TAG, message)
                main.post { events?.success(mapOf("type" to "log", "message" to message)) }
            },
            onState = { state ->
                main.post { events?.success(mapOf("type" to "radio_state", "state" to state)) }
            },
            onRssi = { device, rssi ->
                main.post {
                    events?.success(
                        mapOf("type" to "rssi", "device" to device, "rssi" to rssi)
                    )
                }
            },
        )
        ble = created
        return created
    }

    override fun onDestroy() {
        ble?.stop()
        multicastLock?.let { if (it.isHeld) it.release() }
        multicastLock = null
        super.onDestroy()
    }
}

private fun ByteArray.toHex(): String {
    val chars = "0123456789abcdef"
    val out = CharArray(size * 2)
    for (i in indices) {
        val v = this[i].toInt() and 0xFF
        out[i * 2] = chars[v ushr 4]
        out[i * 2 + 1] = chars[v and 0x0F]
    }
    return String(out)
}

private fun String.hexToBytes(): ByteArray {
    val clean = if (length % 2 == 0) this else ""
    val out = ByteArray(clean.length / 2)
    for (i in out.indices) {
        out[i] = ((Character.digit(clean[i * 2], 16) shl 4) +
            Character.digit(clean[i * 2 + 1], 16)).toByte()
    }
    return out
}
