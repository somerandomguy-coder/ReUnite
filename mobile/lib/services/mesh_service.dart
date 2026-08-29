import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:geolocator/geolocator.dart';
import 'package:path_provider/path_provider.dart';

import 'package:permission_handler/permission_handler.dart';

import '../bridge/ble_radio.dart';
import '../bridge/mesh_ffi.dart';
import '../models/mesh_models.dart';

/// Which radio the mesh is running on.
enum MeshTransport {
  /// UDP over Wi-Fi. Works laptop-to-laptop and phone-to-phone, but needs a shared
  /// network or hotspot.
  wifi,

  /// Bluetooth Low Energy via the native plugin. Needs no infrastructure at all.
  bluetooth,
}

/// The app's single connection to the mesh.
///
/// Every mesh decision - routing, encryption, peer ranking, zone consensus, ghost
/// detection - happens in the Rust core. This class starts that core, pushes commands
/// into it, drains its event stream, and republishes the result to the widgets. It
/// contains no protocol logic, and must not grow any: anything that looks like a rule
/// about the mesh belongs in `crates/meshcore`.
class MeshService extends ChangeNotifier {
  final MeshFfi _ffi;
  final BleRadio _ble;
  MeshService({MeshFfi? ffi, BleRadio? ble})
      : _ffi = ffi ?? MeshFfi.instance,
        _ble = ble ?? BleRadio();

  Timer? _eventTimer;
  Timer? _refreshTimer;
  Timer? _bleTimer;
  StreamSubscription<Map<String, dynamic>>? _bleEvents;
  MeshTransport _transport = MeshTransport.wifi;
  int _bleConnected = 0;
  String? _bleError;

  bool _started = false;
  String? _startError;
  Whoami? _me;
  List<Peer> _peers = const [];
  List<Zone> _zones = const [];
  List<NetworkInfo> _networks = const [];
  List<StatusCode> _statusCodes = const [];
  final List<ChatMessage> _messages = [];

  bool get started => _started;
  String? get startError => _startError;
  MeshTransport get transport => _transport;
  bool get bluetoothAvailable => BleRadio.isAvailable;

  /// Peers the radio can actually reach right now. Zero on Bluetooth means nothing is
  /// connected yet, which is the single most useful thing to show while someone waits.
  int get bleConnected => _bleConnected;
  String? get bleError => _bleError;
  Whoami? get me => _me;
  String get nodeId => _me?.id ?? '...';
  String get activeNetwork => _me?.network ?? 'default';
  bool get sosActive => _me?.sos ?? false;
  int? get myStatus => _me?.status;

  /// Already ranked nearest-first by the core, ghosts last.
  List<Peer> get peers => List.unmodifiable(_peers);
  List<Peer> get livePeers => _peers.where((p) => !p.ghost).toList();
  List<Peer> get ghosts => _peers.where((p) => p.ghost).toList();
  List<Peer> get sosPeers => _peers.where((p) => p.sos).toList();
  List<Zone> get zones => List.unmodifiable(_zones);
  List<NetworkInfo> get networks => List.unmodifiable(_networks);
  List<StatusCode> get statusCodes => List.unmodifiable(_statusCodes);
  List<ChatMessage> get messages => List.unmodifiable(_messages);

  // ------------------------------------------------------------------ lifecycle

  /// Zero-config onboarding (plan.md §2): generate an identity, join `[default]`, done.
  /// No account, no sign-up, and nothing here touches the internet.
  /// [homeOverride], [port], [multicast] and [broadcast] exist so tests can start a real
  /// node without colliding with a node already running on this machine.
  Timer? _autoGpsTimer;

  Future<void> init({
    List<String> peers = const [],
    String? homeOverride,
    int port = 47474,
    bool? multicast,
    bool broadcast = true,
    String? name,
    MeshTransport transport = MeshTransport.bluetooth,
  }) async {
    if (_started) return;
    _transport = transport;
    try {
      if (transport == MeshTransport.bluetooth) {
        final denied = await _requestBluetoothPermissions();
        if (denied != null) {
          _startError = denied;
          notifyListeners();
          return;
        }
      }
      final String homePath;
      if (homeOverride != null) {
        homePath = homeOverride;
      } else {
        final dir = await getApplicationSupportDirectory();
        homePath = '${dir.path}/reunite';
      }
      final home = Directory(homePath);
      if (!home.existsSync()) home.createSync(recursive: true);

      final reply = _ffi.start({
        'home': home.path,
        'transport': transport == MeshTransport.bluetooth ? 'ble' : 'udp',
        'name': name ?? _defaultName(),
        'port': port,
        'peers': peers,
        'multicast': multicast ?? !Platform.isIOS,
        'broadcast': broadcast,
      });

      if (reply['type'] == 'error') {
        _startError = reply['message'] as String? ?? 'unknown error';
        notifyListeners();
        return;
      }
      _me = Whoami.fromJson(reply['whoami'] as Map<String, dynamic>);
      _statusCodes = _ffi.statusTable().map(StatusCode.fromJson).toList();
      _started = true;
      _startError = null;

      _eventTimer = Timer.periodic(const Duration(milliseconds: 200), (_) => _drainEvents());
      _refreshTimer = Timer.periodic(const Duration(seconds: 3), (_) => refresh());
      _autoGpsTimer = Timer.periodic(const Duration(minutes: 2), (_) => _autoLogSafePlace());

      if (transport == MeshTransport.bluetooth) await _startBluetooth();
      refresh();
      _autoLogSafePlace();
    } catch (e) {
      _startError = '$e';
    }
    notifyListeners();
  }

  Future<void> _autoLogSafePlace() async {
    if (!_started) return;
    final fix = await _currentFix();
    if (fix != null) {
      reportZone(fix.$1, fix.$2, 4); // 4 = Safe Level
      _add(ChatKind.notice, 'auto-gps', '🟢 Safe Place logged to map (${fix.$1.toStringAsFixed(4)}, ${fix.$2.toStringAsFixed(4)})');
    }
  }

  String _defaultName() {
    if (Platform.isAndroid) return 'android';
    if (Platform.isIOS) return 'iphone';
    if (Platform.isMacOS) return 'mac';
    return 'reunite';
  }

  @override
  void dispose() {
    _eventTimer?.cancel();
    _refreshTimer?.cancel();
    _autoGpsTimer?.cancel();
    _bleTimer?.cancel();
    _bleEvents?.cancel();
    super.dispose();
  }

  /// Stop the node and start it again on the other radio.
  ///
  /// The identity, contacts, networks and zones all live on disk, so nothing is lost:
  /// the same node comes back on a different transport.
  Future<void> switchTransport(MeshTransport to) async {
    if (to == _transport && _started) return;
    _bleTimer?.cancel();
    await _bleEvents?.cancel();
    _bleEvents = null;
    await _ble.stop();
    _eventTimer?.cancel();
    _refreshTimer?.cancel();
    _ffi.stop();
    _started = false;
    _startError = null;
    _bleError = null;
    _bleConnected = 0;
    _peers = const [];
    notifyListeners();
    await init(transport: to);
  }

  // ------------------------------------------------------------------ bluetooth

  /// Android 12+ gates scanning, advertising and connecting behind separate runtime
  /// permissions, and refuses silently without them.
  Future<String?> _requestBluetoothPermissions() async {
    if (!Platform.isAndroid && !Platform.isIOS && !Platform.isMacOS) {
      return 'Bluetooth mesh is only available on Android, iOS and macOS';
    }
    // macOS asks for Bluetooth authorization itself, via CoreBluetooth, the first time
    // it scans - gated by NSBluetoothAlwaysUsageDescription in Info.plist. permission_handler
    // has no macOS implementation to call here.
    if (Platform.isMacOS) return null;
    try {
      final wanted = Platform.isAndroid
          ? [Permission.bluetoothScan, Permission.bluetoothAdvertise, Permission.bluetoothConnect]
          : [Permission.bluetooth];
      final results = await wanted.request();
      final refused = results.entries.where((e) => !e.value.isGranted).map((e) => e.key);
      if (refused.isNotEmpty) {
        return 'Bluetooth permission was refused (${refused.map((p) => p.toString().split('.').last).join(', ')}). '
            'The mesh cannot see other phones without it.';
      }
      return null;
    } catch (e) {
      return 'could not request Bluetooth permission: $e';
    }
  }

  Future<void> _startBluetooth() async {
    if (!await _ble.isSupported()) {
      _bleError = 'this device has no Bluetooth LE radio';
      return;
    }
    if (!await _ble.isEnabled()) {
      _bleError = 'Bluetooth is turned off - switch it on to reach other phones';
      return;
    }
    final err = await _ble.start();
    if (err != null) {
      _bleError = err;
      return;
    }
    _bleError = null;

    // Frames arriving off the air go straight into the core.
    _bleEvents = _ble.events().listen((event) {
      switch (event['type'] as String?) {
        case 'frame':
          _ffi.bleInject(event['frame'] as String, event['from'] as String);
          break;
        case 'peer_lost':
          _ffi.blePeerLost(event['device'] as String);
          break;
        case 'log':
          debugPrint('BLE: ${event['message']}');
          break;
      }
    }, onError: (Object e) => debugPrint('BLE event stream error: $e'));

    // ...and frames the core wants sent go out to the radio. 100ms keeps beacons
    // punctual without waking the radio pointlessly.
    _bleTimer = Timer.periodic(const Duration(milliseconds: 100), (_) => _pumpBluetooth());
  }

  Future<void> _pumpBluetooth() async {
    if (!_started) return;
    final pending = _ffi.bleDrain();
    for (final item in pending) {
      await _ble.send(item['frame'] as String, item['to'] as String?);
    }
    final connected = await _ble.connectedCount();
    if (connected != _bleConnected) {
      _bleConnected = connected;
      notifyListeners();
    }
  }

  // -------------------------------------------------------------------- plumbing

  Map<String, dynamic> _call(Map<String, dynamic> cmd) {
    if (!_started) return {'type': 'error', 'message': 'mesh not started'};
    return _ffi.command(cmd);
  }

  /// Pull everything the screens display. Cheap: these are in-memory reads in Rust.
  void refresh() {
    if (!_started) return;
    final p = _call({'cmd': 'peers'});
    if (p['type'] == 'peers') {
      _peers = (p['peers'] as List).map((e) => Peer.fromJson(e as Map<String, dynamic>)).toList();
    }
    final z = _call({'cmd': 'heatmap'});
    if (z['type'] == 'heatmap') {
      _zones = (z['zones'] as List).map((e) => Zone.fromJson(e as Map<String, dynamic>)).toList();
    }
    final n = _call({'cmd': 'networks'});
    if (n['type'] == 'networks') {
      _networks =
          (n['networks'] as List).map((e) => NetworkInfo.fromJson(e as Map<String, dynamic>)).toList();
    }
    final w = _call({'cmd': 'whoami'});
    if (w['type'] == 'whoami') {
      _me = Whoami.fromJson(w['whoami'] as Map<String, dynamic>);
    }
    notifyListeners();
  }

  void _drainEvents() {
    if (!_started) return;
    var changed = false;
    // Bounded so a burst can never starve the frame.
    for (var i = 0; i < 64; i++) {
      final event = _ffi.pollEvent(0);
      if (event == null) break;
      _handleEvent(event);
      changed = true;
    }
    if (changed) notifyListeners();
  }

  void _handleEvent(Map<String, dynamic> e) {
    switch (e['type'] as String?) {
      case 'chat':
        _add(ChatKind.chat, e['from'] as String, e['text'] as String,
            fromId: e['from_id'] as String?, network: e['network'] as String?, hops: e['hops'] as int?);
        break;
      case 'direct':
        _add(ChatKind.direct, e['from'] as String, e['text'] as String,
            fromId: e['from_id'] as String?, network: e['network'] as String?, hops: e['hops'] as int?);
        break;
      case 'sos_raised':
        _add(ChatKind.sos, e['display'] as String,
            'SOS - mesh alert only, emergency services were NOT called');
        break;
      case 'sos_cleared':
        _add(ChatKind.notice, e['display'] as String, 'cleared their SOS');
        break;
      case 'status_update':
        _add(ChatKind.status, e['display'] as String, describeStatus(e['code'] as int));
        break;
      case 'zone_update':
        _add(ChatKind.notice, e['from'] as String,
            'reported a zone: ${(e['level_scaled'] as num).toStringAsFixed(1)}/4 safe, '
            '${e['consensus']} verifying');
        break;
      case 'peer_joined':
        _add(ChatKind.notice, e['display'] as String, 'is in range');
        break;
      case 'peer_lost':
        _add(ChatKind.notice, e['display'] as String, 'went quiet');
        break;
      case 'location_update':
        final d = e['distance_m'] as num?;
        _add(ChatKind.notice, e['display'] as String,
            'shared a position${d == null ? '' : ' (${formatDistance(d.toDouble())} away)'}');
        break;
      case 'delivered':
        _add(ChatKind.notice, 'you', 'delivered to ${e['to']}');
        break;
      case 'notice':
        _add(ChatKind.notice, 'mesh', e['text'] as String);
        break;
      case 'warning':
        _add(ChatKind.warning, 'mesh', e['text'] as String);
        break;
      case 'context':
        refresh();
        break;
    }
  }

  void _add(ChatKind kind, String from, String text,
      {String? fromId, String? network, int? hops}) {
    _messages.add(ChatMessage(
      kind: kind,
      from: from,
      fromId: fromId,
      text: text,
      network: network ?? activeNetwork,
      hops: hops,
    ));
    // Keep the log bounded - a long-running node in a busy mesh must not grow forever.
    if (_messages.length > 500) _messages.removeRange(0, _messages.length - 500);
  }

  // -------------------------------------------------------------------- commands

  String? _ok(Map<String, dynamic> reply) =>
      reply['type'] == 'error' ? reply['message'] as String? : null;

  /// Broadcast to the active network. Returns an error string, or null on success.
  String? sendMessage(String text) {
    if (text.trim().isEmpty) return null;
    final err = _ok(_call({'cmd': 'broadcast', 'text': text}));
    if (err == null) _add(ChatKind.mine, 'you', text);
    notifyListeners();
    return err;
  }

  String? sendDirect(String target, String text) {
    final err = _ok(_call({'cmd': 'direct', 'target': target, 'text': text}));
    if (err == null) _add(ChatKind.mine, 'you', '-> $target: $text');
    notifyListeners();
    return err;
  }

  /// Raise or clear the in-network SOS.
  ///
  /// This alerts the mesh around you and nothing else. plan.md §3.2 isolates it from the
  /// operating system's emergency-call path on purpose, so that testing the app can never
  /// dial real emergency services. Do not wire this to a phone dialler.
  String? setSos(bool active) {
    final err = _ok(_call({'cmd': 'sos', 'on': active}));
    refresh();
    return err;
  }

  /// Send a pre-canned panic message. One byte on the wire.
  String? setStatus(int code) {
    final err = _ok(_call({'cmd': 'set_status', 'code': code}));
    if (err == null) _add(ChatKind.mine, 'you', describeStatus(code));
    refresh();
    return err;
  }

  String describeStatus(int code) {
    for (final s in _statusCodes) {
      if (s.code == code) return s.text;
    }
    return code == 0 ? 'status cleared' : 'status $code';
  }

  String? reportZone(double lat, double lon, int level) {
    final err = _ok(_call({'cmd': 'report_zone', 'lat': lat, 'lon': lon, 'level': level}));
    refresh();
    return err;
  }

  String? createNetwork(String name) {
    final err = _ok(_call({'cmd': 'create_network', 'name': name}));
    refresh();
    return err;
  }

  String? invite(String network, String user) {
    final err = _ok(_call({'cmd': 'invite', 'network': network, 'user': user}));
    refresh();
    return err;
  }

  String? switchNetwork(String name) {
    final err = _ok(_call({'cmd': 'switch', 'name': name}));
    refresh();
    return err;
  }

  String? setStoring(String network, bool on) {
    final err = _ok(_call({'cmd': 'set_storing', 'network': network, 'on': on}));
    refresh();
    return err;
  }

  String? kick(String user) {
    final err = _ok(_call({'cmd': 'kick', 'user': user}));
    refresh();
    return err;
  }

  String? rename(String user, String name) {
    final err = _ok(_call({'cmd': 'rename', 'user': user, 'name': name}));
    refresh();
    return err;
  }

  /// Read the GPS and publish it to the mesh.
  Future<String?> shareCurrentLocation() async {
    final fix = await _currentFix();
    if (fix == null) {
      return 'no GPS fix - grant location permission, or set a position manually';
    }
    final err = _ok(_call({'cmd': 'set_location', 'lat': fix.$1, 'lon': fix.$2}));
    if (err != null) return err;
    final shared = _ok(_call({'cmd': 'share_location'}));
    refresh();
    return shared;
  }

  String? setLocation(double lat, double lon) {
    final err = _ok(_call({'cmd': 'set_location', 'lat': lat, 'lon': lon}));
    refresh();
    return err;
  }

  /// Report the safety of wherever we are standing.
  Future<String?> reportZoneHere(int level) async {
    final fix = await _currentFix();
    if (fix == null) return 'no GPS fix - cannot report a zone without a position';
    return reportZone(fix.$1, fix.$2, level);
  }

  Future<(double, double)?> _currentFix() async {
    try {
      if (!await Geolocator.isLocationServiceEnabled()) {
        // Desktop and simulators often have no location service; fall back to whatever
        // position the user already set, so the feature still works for testing.
        final me = _me;
        if (me?.lat != null && me?.lon != null) return (me!.lat!, me.lon!);
        return null;
      }
      var permission = await Geolocator.checkPermission();
      if (permission == LocationPermission.denied) {
        permission = await Geolocator.requestPermission();
      }
      if (permission == LocationPermission.denied ||
          permission == LocationPermission.deniedForever) {
        final me = _me;
        if (me?.lat != null && me?.lon != null) return (me!.lat!, me.lon!);
        return null;
      }
      final pos = await Geolocator.getCurrentPosition(
        desiredAccuracy: LocationAccuracy.high,
      );
      return (pos.latitude, pos.longitude);
    } catch (e) {
      debugPrint('GPS unavailable: $e');
      final me = _me;
      if (me?.lat != null && me?.lon != null) return (me!.lat!, me.lon!);
      return null;
    }
  }
}

String formatDistance(double metres) =>
    metres < 1000 ? '${metres.toStringAsFixed(0)}m' : '${(metres / 1000).toStringAsFixed(2)}km';

String formatAge(int ms) {
  if (ms < 1000) return 'just now';
  if (ms < 60000) return '${ms ~/ 1000}s ago';
  if (ms < 3600000) return '${ms ~/ 60000}m ago';
  return '${ms ~/ 3600000}h ago';
}
