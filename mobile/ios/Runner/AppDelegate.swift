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
          result(self.ble?.isEnabled ?? false)
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
