import CoreBluetooth
import Flutter
import UIKit

/// Wires the Bluetooth radio to Dart, mirroring `MainActivity.kt` on Android.
///
/// Frames cross as hex strings and are never inspected here - every protocol decision
/// stays in the Rust core.
@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private static let methodChannel = "reunite/ble"
  private static let eventChannel = "reunite/ble/events"

  private var ble: BleMesh?
  private var events: FlutterEventSink?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    let controller = window?.rootViewController as? FlutterViewController
    if let messenger = controller?.binaryMessenger {
      register(with: messenger)
    }
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    // The application registrar is where an app-level (non-plugin) channel gets its
    // binary messenger from.
    register(with: engineBridge.applicationRegistrar.messenger())
  }

  private func register(with messenger: FlutterBinaryMessenger) {
    FlutterEventChannel(name: Self.eventChannel, binaryMessenger: messenger)
      .setStreamHandler(self)

    FlutterMethodChannel(name: Self.methodChannel, binaryMessenger: messenger)
      .setMethodCallHandler { [weak self] call, result in
        guard let self else { return }
        switch call.method {
        case "isSupported":
          result(true)
        case "isEnabled":
          // Through `radio()`, not `self.ble?`. Reading the optional returned false on
          // every fresh launch - the radio is not constructed until something asks for
          // it - and Dart treated that as "Bluetooth is off" and refused to start,
          // telling the user to switch on a radio that was already on. That single
          // `?.` is why an iPhone never advertised or scanned.
          result(self.radio().isEnabled)
        case "state":
          // Best current knowledge. Before CoreBluetooth has reported in this is
          // "unknown", which is the honest answer and is what stops the UI asserting a
          // cause it has not established.
          result(self.ble.map { BleMesh.describe($0.currentState) } ?? "unknown")
        case "start":
          if let error = self.radio().start() {
            result(FlutterError(code: "BLE_START_FAILED", message: error, details: nil))
          } else {
            result(true)
          }
        case "stop":
          self.ble?.stop()
          result(true)
        case "connectedCount":
          result(self.ble?.connectedCount ?? 0)
        case "setCadence":
          let args = call.arguments as? [String: Any] ?? [:]
          self.radio().setCadence(
            scan: args["scan"] as? String ?? "low_latency",
            windowMs: args["windowMs"] as? Int,
            periodMs: args["periodMs"] as? Int
          )
          result(true)
        case "send":
          guard let args = call.arguments as? [String: Any],
                let frame = args["frame"] as? String
          else {
            result(FlutterError(code: "BAD_ARGS", message: "missing 'frame'", details: nil))
            return
          }
          result(self.radio().send(frameHex: frame, to: args["to"] as? String))
        default:
          result(FlutterMethodNotImplemented)
        }
      }
  }

  private func radio() -> BleMesh {
    if let existing = ble { return existing }
    let created = BleMesh(
      onFrame: { [weak self] frameHex, device in
        // Platform channels are main-thread only.
        DispatchQueue.main.async {
          self?.events?(["type": "frame", "frame": frameHex, "from": device])
        }
      },
      onPeerLost: { [weak self] device in
        DispatchQueue.main.async {
          self?.events?(["type": "peer_lost", "device": device])
        }
      },
      onLog: { [weak self] message in
        NSLog("ReUnite BLE: %@", message)
        DispatchQueue.main.async {
          self?.events?(["type": "log", "message": message])
        }
      },
      onState: { [weak self] state in
        // The only authoritative answer to "is Bluetooth on". CoreBluetooth delivers it
        // asynchronously and never before a manager exists, so it is pushed rather than
        // polled - any synchronous answer before this point is a guess.
        DispatchQueue.main.async {
          self?.events?(["type": "radio_state", "state": state])
        }
      },
      onRssi: { [weak self] device, rssi in
        DispatchQueue.main.async {
          self?.events?(["type": "rssi", "device": device, "rssi": rssi])
        }
      }
    )
    ble = created
    return created
  }
}

extension AppDelegate: FlutterStreamHandler {
  func onListen(
    withArguments arguments: Any?,
    eventSink events: @escaping FlutterEventSink
  ) -> FlutterError? {
    self.events = events
    return nil
  }

  func onCancel(withArguments arguments: Any?) -> FlutterError? {
    events = nil
    return nil
  }
}
