import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';

// C-FFI Signature Definitions
typedef NativeMeshInit = Pointer<Utf8> Function(Pointer<Utf8> homePath, Pointer<Utf8> name);
typedef DartMeshInit = Pointer<Utf8> Function(Pointer<Utf8> homePath, Pointer<Utf8> name);

typedef NativeMeshSend = Pointer<Utf8> Function(Pointer<Utf8> cmdStr);
typedef DartMeshSend = Pointer<Utf8> Function(Pointer<Utf8> cmdStr);

typedef NativeMeshPoll = Pointer<Utf8> Function();
typedef DartMeshPoll = Pointer<Utf8> Function();

typedef NativeStringFree = Void Function(Pointer<Utf8> ptr);
typedef DartStringFree = void Function(Pointer<Utf8> ptr);

/// High-Level Flutter Dart FFI Bridge connecting Flutter to Rust meshcore engine
class RustMeshFFI {
  static DynamicLibrary? _lib;
  static DartMeshInit? _initFn;
  static DartMeshSend? _sendFn;
  static DartMeshPoll? _pollFn;
  static DartStringFree? _freeFn;

  static bool _isLoaded = false;
  static bool get isLoaded => _isLoaded;

  /// Load the compiled native C library (.so for Android, .dylib for macOS/iOS, .so for Linux)
  static bool loadLibrary() {
    if (_isLoaded) return true;

    try {
      if (Platform.isAndroid) {
        _lib = DynamicLibrary.open("libmeshcore.so");
      } else if (Platform.isLinux) {
        // Fallback to local build artifact for Linux testing
        const path = "target/debug/libmeshcore.so";
        if (File(path).existsSync()) {
          _lib = DynamicLibrary.open(path);
        } else {
          _lib = DynamicLibrary.process();
        }
      } else if (Platform.isMacOS) {
        // scripts/build_ffi.sh macos installs the dylib here.
        final home = Platform.environment['HOME'];
        final path = home == null ? null : '$home/.reunite/lib/libmeshffi.dylib';
        if (path != null && File(path).existsSync()) {
          _lib = DynamicLibrary.open(path);
        }
      } else if (Platform.isIOS) {
        // scripts/build_ffi.sh ios produces a static library linked directly into
        // the app binary (see docs/MOBILE.md), so its symbols are already in-process.
        _lib = DynamicLibrary.process();
      }

      if (_lib != null) {
        _initFn = _lib!.lookupFunction<NativeMeshInit, DartMeshInit>("mesh_node_init");
        _sendFn = _lib!.lookupFunction<NativeMeshSend, DartMeshSend>("mesh_node_send_command");
        _pollFn = _lib!.lookupFunction<NativeMeshPoll, DartMeshPoll>("mesh_node_poll_event");
        _freeFn = _lib!.lookupFunction<NativeStringFree, DartStringFree>("mesh_string_free");
        _isLoaded = true;
        debugPrint("[RustMeshFFI] Native Rust meshcore library linked successfully!");
        return true;
      }
    } catch (e) {
      debugPrint("[RustMeshFFI] Native library linking warning (fallback active): $e");
    }
    return false;
  }

  /// Initialize the native Rust mesh actor
  static Map<String, dynamic>? initNode(String homePath, String name) {
    if (!loadLibrary() || _initFn == null || _freeFn == null) return null;

    final homePtr = homePath.toNativeUtf8();
    final namePtr = name.toNativeUtf8();
    final resPtr = _initFn!(homePtr, namePtr);

    malloc.free(homePtr);
    malloc.free(namePtr);

    final resStr = resPtr.toDartString();
    _freeFn!(resPtr);

    try {
      return jsonDecode(resStr);
    } catch (_) {
      return {"raw": resStr};
    }
  }

  /// Send a command to native Rust core
  static Map<String, dynamic>? sendCommand(String cmd) {
    if (!_isLoaded || _sendFn == null || _freeFn == null) return null;

    final cmdPtr = cmd.toNativeUtf8();
    final resPtr = _sendFn!(cmdPtr);
    malloc.free(cmdPtr);

    final resStr = resPtr.toDartString();
    _freeFn!(resPtr);

    try {
      return jsonDecode(resStr);
    } catch (_) {
      return {"raw": resStr};
    }
  }

  /// Poll incoming events from Rust meshcore event queue
  static Map<String, dynamic>? pollEvent() {
    if (!_isLoaded || _pollFn == null || _freeFn == null) return null;

    final resPtr = _pollFn!();
    final resStr = resPtr.toDartString();
    _freeFn!(resPtr);

    if (resStr.contains('"status":"empty"')) return null;

    try {
      return jsonDecode(resStr);
    } catch (_) {
      return {"raw": resStr};
    }
  }
}
