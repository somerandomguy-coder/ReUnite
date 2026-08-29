import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// The native Bluetooth radio, as seen from Dart.
///
/// The actual BLE work - advertising, scanning, GATT, chunking - is Kotlin
/// (`BleMesh.kt`) and Swift (`BleMesh.swift`), because mobile operating systems will not
/// let a Rust library own their Bluetooth stack. This class is only the wire between
/// that native code and the mesh core: frames go across as hex and are never inspected
/// here.
class BleRadio {
  static const _method = MethodChannel('reunite/ble');
  static const _events = EventChannel('reunite/ble/events');

  /// Bluetooth is only wired up on the two platforms that have a native implementation.
  static bool get isAvailable => Platform.isAndroid || Platform.isIOS;

  Future<bool> isSupported() async {
    if (!isAvailable) return false;
    try {
      return await _method.invokeMethod<bool>('isSupported') ?? false;
    } on PlatformException catch (e) {
      debugPrint('BLE isSupported failed: ${e.message}');
      return false;
    }
  }

  /// Whether the radio is powered on, **as far as the platform currently knows**.
  ///
  /// On iOS this is unanswerable until CoreBluetooth has reported in, which it does not
  /// do until a manager exists. Treating a pre-start answer as authoritative is the bug
  /// that kept the iPhone off the mesh entirely: it always read false, and the app
  /// refused to start the radio rather than letting the platform tell it otherwise.
  /// Prefer the asynchronous `radio_state` event; use this only for a display hint.
  Future<bool> isEnabled() async {
    if (!isAvailable) return false;
    try {
      return await _method.invokeMethod<bool>('isEnabled') ?? false;
    } on PlatformException {
      return false;
    }
  }

  /// Best current knowledge of the radio state: `on`, `off`, `unauthorized`,
  /// `unsupported`, `resetting` or `unknown`.
  Future<String> state() async {
    if (!isAvailable) return 'unsupported';
    try {
      return await _method.invokeMethod<String>('state') ?? 'unknown';
    } on PlatformException {
      return 'unknown';
    }
  }

  /// Start advertising and scanning. Returns an error string, or null on success.
  Future<String?> start() async {
    if (!isAvailable) return 'Bluetooth mesh is only available on Android and iOS';
    try {
      await _method.invokeMethod<bool>('start');
      return null;
    } on PlatformException catch (e) {
      return e.message ?? 'could not start Bluetooth';
    }
  }

  Future<void> stop() async {
    if (!isAvailable) return;
    try {
      await _method.invokeMethod<bool>('stop');
    } on PlatformException catch (e) {
      debugPrint('BLE stop failed: ${e.message}');
    }
  }

  /// How many peers a frame could actually reach right now.
  Future<int> connectedCount() async {
    if (!isAvailable) return 0;
    try {
      return await _method.invokeMethod<int>('connectedCount') ?? 0;
    } on PlatformException {
      return 0;
    }
  }

  /// Put one frame on the air. [to] names a device, or null to reach every peer.
  /// Returns how many peers it was sent to.
  Future<int> send(String frameHex, String? to) async {
    if (!isAvailable) return 0;
    try {
      return await _method.invokeMethod<int>('send', {'frame': frameHex, 'to': to}) ?? 0;
    } on PlatformException catch (e) {
      debugPrint('BLE send failed: ${e.message}');
      return 0;
    }
  }

  /// Tell the radio how hard to listen (phase 2D).
  ///
  /// [scan] is `low_latency`, `balanced` or `low_power`. A non-null [windowMs] and
  /// [periodMs] ask for a windowed scan - listen for the window, sleep out the period -
  /// which is what actually saves the battery once nobody is around.
  Future<void> setCadence({
    required String scan,
    int? windowMs,
    int? periodMs,
  }) async {
    if (!isAvailable) return;
    try {
      await _method.invokeMethod<void>('setCadence', {
        'scan': scan,
        'windowMs': windowMs,
        'periodMs': periodMs,
      });
    } on PlatformException catch (e) {
      debugPrint('BLE setCadence failed: ${e.message}');
    }
  }

  /// Frames arriving off the air, plus peer-loss and log notices.
  Stream<Map<String, dynamic>> events() {
    if (!isAvailable) return const Stream.empty();
    return _events.receiveBroadcastStream().map((e) => Map<String, dynamic>.from(e as Map));
  }
}
