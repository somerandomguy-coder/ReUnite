import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';

/// Raw `dart:ffi` bindings to the Rust mesh core (`crates/meshffi`).
///
/// The whole contract is JSON in, JSON out, over five C functions. Nothing here knows
/// anything about the mesh protocol - that all lives in Rust, and this file exists only
/// to move strings across the boundary and free them again.
typedef _StartNative = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _StartDart = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _CommandNative = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _CommandDart = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _PollNative = Pointer<Utf8> Function(Uint64);
typedef _PollDart = Pointer<Utf8> Function(int);
typedef _FreeNative = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);
typedef _TableNative = Pointer<Utf8> Function();
typedef _TableDart = Pointer<Utf8> Function();
typedef _DrainNative = Pointer<Utf8> Function();
typedef _DrainDart = Pointer<Utf8> Function();
typedef _LostNative = Void Function(Pointer<Utf8>);
typedef _LostDart = void Function(Pointer<Utf8>);
typedef _RssiNative = Void Function(Pointer<Utf8>);
typedef _RssiDart = void Function(Pointer<Utf8>);
typedef _StopNative = Bool Function();
typedef _StopDart = bool Function();

class MeshFfiException implements Exception {
  final String message;
  MeshFfiException(this.message);
  @override
  String toString() => 'MeshFfiException: $message';
}

class MeshFfi {
  final DynamicLibrary _lib;

  /// True when this instance is the do-nothing stub built because the core would not
  /// load. Callers must check it before trusting any reply: the stub returns null for
  /// every call, which decodes to an empty, meaningless reply.
  final bool isStub;
  late final _StartDart _start;
  late final _CommandDart _command;
  late final _PollDart _poll;
  late final _FreeDart _free;
  late final _TableDart _table;
  late final _DrainDart _bleDrain;
  late final _CommandDart _bleInject;
  late final _LostDart _blePeerLost;
  late final _RssiDart _bleRssi;
  late final _StopDart _stop;

  MeshFfi._(this._lib) : isStub = false {
    _start = _lib.lookupFunction<_StartNative, _StartDart>('mesh_start');
    _command = _lib.lookupFunction<_CommandNative, _CommandDart>('mesh_command');
    _poll = _lib.lookupFunction<_PollNative, _PollDart>('mesh_poll_event');
    _free = _lib.lookupFunction<_FreeNative, _FreeDart>('mesh_free');
    _table = _lib.lookupFunction<_TableNative, _TableDart>('mesh_status_table');
    _bleDrain = _lib.lookupFunction<_DrainNative, _DrainDart>('mesh_ble_drain');
    _bleInject = _lib.lookupFunction<_CommandNative, _CommandDart>('mesh_ble_inject');
    _blePeerLost = _lib.lookupFunction<_LostNative, _LostDart>('mesh_ble_peer_lost');
    _bleRssi = _lib.lookupFunction<_RssiNative, _RssiDart>('mesh_ble_rssi');
    _stop = _lib.lookupFunction<_StopNative, _StopDart>('mesh_stop');
  }

  static MeshFfi? _instance;
  static bool _hasLoadedNative = false;
  static String? _loadError;

  /// False when the core could not be loaded and every call is going to the stub.
  static bool get nativeLoaded => _hasLoadedNative;

  /// Why the core could not be loaded, or null when it did load.
  ///
  /// `_open()` builds a precise message naming every path it tried. Without this getter
  /// that message was thrown away and the stub's null replies surfaced to the user as
  /// "empty reply from core" - a sentence that describes the symptom and hides the cause.
  static String? get loadError => _loadError;

  static MeshFfi get instance {
    if (_instance == null) {
      try {
        _instance = MeshFfi._(_open());
        _hasLoadedNative = true;
        _loadError = null;
      } catch (e) {
        debugPrint('[MeshFfi] Native C-library loading warning: $e');
        _instance = MeshFfi._mock();
        _hasLoadedNative = false;
        _loadError = e is MeshFfiException ? e.message : '$e';
      }
    }
    return _instance!;
  }

  MeshFfi._mock()
      : _lib = DynamicLibrary.process(),
        isStub = true {
    _start = (_) => nullptr;
    _command = (_) => nullptr;
    _poll = (_) => nullptr;
    _free = (_) => {};
    _table = () => nullptr;
    _bleDrain = () => nullptr;
    _bleInject = (_) => nullptr;
    _blePeerLost = (_) => {};
    _bleRssi = (_) => {};
    _stop = () => true;
  }

  /// Where the compiled core might be, most specific first.
  static DynamicLibrary _open() {
    final override = Platform.environment['MESHFFI_LIB'];
    if (override != null && override.isNotEmpty) {
      return DynamicLibrary.open(override);
    }
    if (Platform.isIOS) return DynamicLibrary.process();
    if (Platform.isAndroid) {
      try {
        return DynamicLibrary.open('libmeshffi.so');
      } catch (e) {
        throw MeshFfiException('libmeshffi.so dlopen failed on Android: $e');
      }
    }

    final home = Platform.environment['HOME'] ?? '';
    final name = Platform.isMacOS ? 'libmeshffi.dylib' : 'libmeshffi.so';
    final candidates = <String>[
      name,
      '$home/.reunite/lib/$name',
      '${Directory.current.path}/../target/release/$name',
      '${Directory.current.path}/target/release/$name',
    ];
    final failures = <String>[];
    for (final path in candidates) {
      try {
        return DynamicLibrary.open(path);
      } on ArgumentError catch (e) {
        failures.add('$path -> $e');
      }
    }
    throw MeshFfiException(
      'could not load the mesh core.\nTried:\n  ${failures.join('\n  ')}\n\n'
      'Run scripts/build_ffi.sh for your platform, or set MESHFFI_LIB to the library path.',
    );
  }

  /// Take ownership of a string the Rust side allocated, then hand the memory back.
  String _consume(Pointer<Utf8> ptr) {
    if (ptr == nullptr) return '';
    try {
      return ptr.toDartString();
    } finally {
      _free(ptr);
    }
  }

  Map<String, dynamic> _json(String text) {
    if (text.isEmpty) return {'type': 'error', 'message': 'empty reply from core'};
    return jsonDecode(text) as Map<String, dynamic>;
  }

  /// Start the node. Returns the `whoami` reply, or an `error` reply.
  Map<String, dynamic> start(Map<String, dynamic> config) {
    final input = jsonEncode(config).toNativeUtf8();
    try {
      return _json(_consume(_start(input)));
    } finally {
      calloc.free(input);
    }
  }

  /// Run one command and get its reply.
  Map<String, dynamic> command(Map<String, dynamic> cmd) {
    final input = jsonEncode(cmd).toNativeUtf8();
    try {
      return _json(_consume(_command(input)));
    } finally {
      calloc.free(input);
    }
  }

  /// Block for up to [timeoutMs] waiting for the next event. Null on timeout.
  ///
  /// This blocks the calling thread, so it must only ever be called from a background
  /// isolate - never from the UI isolate, which would freeze the app.
  Map<String, dynamic>? pollEvent(int timeoutMs) {
    final raw = _poll(timeoutMs);
    if (raw == nullptr) return null;
    final text = _consume(raw);
    if (text.isEmpty) return null;
    return jsonDecode(text) as Map<String, dynamic>;
  }

  /// The pre-canned panic codes, read from the core so the UI cannot drift out of sync
  /// with the protocol.
  List<Map<String, dynamic>> statusTable() {
    final text = _consume(_table());
    if (text.isEmpty) return const [];
    return (jsonDecode(text) as List).cast<Map<String, dynamic>>();
  }

  // --------------------------------------------------------------- ble radio

  /// Frames the core wants transmitted. Each is `{frame: hex, to: deviceId?}`, where a
  /// null `to` means "everyone in range". Empty when the node is not on the BLE
  /// transport, so calling this unconditionally is safe.
  List<Map<String, dynamic>> bleDrain() {
    final text = _consume(_bleDrain());
    if (text.isEmpty) return const [];
    return (jsonDecode(text) as List).cast<Map<String, dynamic>>();
  }

  /// Hand the core a frame that arrived over Bluetooth.
  Map<String, dynamic> bleInject(String frameHex, String fromDevice) {
    final input = jsonEncode({'frame': frameHex, 'from': fromDevice}).toNativeUtf8();
    try {
      return _json(_consume(_bleInject(input)));
    } finally {
      calloc.free(input);
    }
  }

  /// A Bluetooth peer disconnected, so its link mapping should be dropped.
  void blePeerLost(String device) {
    final input = device.toNativeUtf8();
    try {
      _blePeerLost(input);
    } finally {
      calloc.free(input);
    }
  }

  /// Tell the core how strong a device's signal was, in dBm.
  ///
  /// Fire-and-forget: this arrives several times a second per device in range, and the
  /// core only keeps the latest. It is deliberately not a `command` round trip.
  void bleRssi(String device, int rssi) {
    final input = jsonEncode({'device': device, 'rssi': rssi}).toNativeUtf8();
    try {
      _bleRssi(input);
    } finally {
      calloc.free(input);
    }
  }

  /// Stop the node and release its port or radio. True if one was running.
  bool stop() => _stop();
}
