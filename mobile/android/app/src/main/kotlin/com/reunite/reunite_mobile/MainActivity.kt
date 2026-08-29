package com.reunite.reunite_mobile

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import android.util.Log
import io.flutter.embedding.android.FlutterActivity

/// Android drops multicast and subnet-broadcast frames unless the app holds a
/// MulticastLock. Without it the mesh silently hears nothing: the socket binds fine, the
/// node beacons happily, and no peer ever appears - which looks like a bug in the mesh
/// rather than a platform default. Hold it for the life of the activity.
class MainActivity : FlutterActivity() {
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            multicastLock = wifi.createMulticastLock("reunite-mesh").apply {
                setReferenceCounted(true)
                acquire()
            }
            Log.i("ReUnite", "multicast lock acquired - mesh discovery can receive")
        } catch (e: Exception) {
            // Not fatal: unicast to an explicitly added peer still works.
            Log.w("ReUnite", "could not acquire multicast lock: ${e.message}")
        }
    }

    override fun onDestroy() {
        multicastLock?.let { if (it.isHeld) it.release() }
        multicastLock = null
        super.onDestroy()
    }
}
