import Cocoa
import FlutterMacOS

/// Wires the Bluetooth radio to Dart, mirroring `AppDelegate.swift` on iOS. macOS plays
/// central role only - see `BleMeshCentral.swift`.
///
/// Frames cross as hex strings and are never inspected here - every protocol decision
/// stays in the Rust core.
@main
class AppDelegate: FlutterAppDelegate {
  private static let methodChannel = "reunite/ble"
  private static let eventChannel = "reunite/ble/events"

  private var ble: BleMesh?
  private var events: FlutterEventSink?

  override func applicationDidFinishLaunching(_ notification: Notification) {
    if let controller = mainFlutterWindow?.contentViewController as? FlutterViewController {
      register(with: controller.engine.binaryMessenger)
    }
    super.applicationDidFinishLaunching(notification)
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
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
