import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:geolocator/geolocator.dart';
import 'package:path_provider/path_provider.dart';

import 'package:permission_handler/permission_handler.dart';

import '../bridge/ble_radio.dart';
import '../bridge/mesh_ffi.dart';
import '../models/mesh_models.dart';

/// The message to show for a radio state the platform reported, or null when there is
/// nothing to say.
///
/// A free function so it can be tested without a radio. The rule it encodes is the one
/// that cost this project its entire iOS Bluetooth support: **only a state the platform
/// actually reported may produce a diagnosis.** `unknown` and `resetting` are the
/// platform declining to answer, and the app must decline too - the previous build
/// turned "I do not know yet" into "Bluetooth is turned off", which sent people to check
/// a setting that was already correct.
String? bleErrorForRadioState(String state) => switch (state) {
      'off' => 'Bluetooth is turned off - switch it on to reach other phones',
      'unauthorized' => 'Bluetooth permission was refused - grant it in Settings',
      'unsupported' => 'this device has no Bluetooth LE radio',
      _ => null,
    };

/// Whether a platform may use the automatic UDP discovery paths - multicast and
/// broadcast - or has to be told each address it is allowed to talk to.
///
/// iOS gets neither, and that is not a preference. Apple gates **both** UDP multicast and
/// UDP broadcast, sending and receiving alike, behind
/// `com.apple.developer.networking.multicast` - a restricted entitlement granted only on
/// application to Apple, which this build does not hold. Frames on those paths are
/// dropped by the OS before they reach the wire, and inbound ones are dropped on receipt.
///
/// Leaving the switches on would not make them work. It would only make the core's own
/// `describe()` announce `broadcast` on a transport that has no such reach - a confident
/// wrong answer, which this codebase treats as worse than no answer at all for exactly
/// the reason spelled out above [bleErrorForRadioState]. What Apple does leave open is
/// plain unicast, which needs only the local-network permission, so on iOS Wi-Fi reaches
/// the addresses it was given and nothing else.
bool udpAutoDiscoveryDefault({required bool isIOS}) => !isIOS;

/// Split a peer list into `host:port` entries, dropping blanks and `#` comments.
///
/// One parser for both routes in: the `--dart-define=MESH_PEERS=a,b` build flag and the
/// saved file, which is the same list one per line.
List<String> parsePeerList(String raw) => raw
    .split('\n')
    // Comments are stripped per line, before the line is split - otherwise the words of
    // a comment survive as peers, which is a silent way to feed the core garbage.
    .map((line) {
      final hash = line.indexOf('#');
      return hash < 0 ? line : line.substring(0, hash);
    })
    .expand((line) => line.split(RegExp(r'[,\s]+')))
    .map((p) => p.trim())
    .where((p) => p.isNotEmpty)
    .toList();

/// Why [value] is not a usable `host:port` peer address, or null when it is.
///
/// Deliberately shallow - it checks the shape, never whether anything is listening.
/// The core parses each address itself and silently drops one it cannot read, so
/// catching the typo here is the difference between being told about it and no feedback
/// at all.
String? peerAddressError(String value) {
  final text = value.trim();
  if (text.isEmpty) return 'enter an address like 10.17.158.195:47474';
  // Last colon, so an IPv6 literal in brackets still splits on its port.
  final colon = text.lastIndexOf(':');
  if (colon <= 0 || colon == text.length - 1) {
    return 'needs a host and a port, like 10.17.158.195:47474';
  }
  final port = int.tryParse(text.substring(colon + 1));
  if (port == null || port < 1 || port > 65535) {
    return 'the part after ":" must be a port number, like 47474';
  }
  return null;
}

/// The JSON handed to `mesh_start`.
///
/// A free function so a test can assert on the exact configuration a platform produces
/// without binding a socket or loading the core - which matters most for iOS, whose
/// configuration cannot be exercised on the machine that builds it.
Map<String, dynamic> startConfigJson({
  required String home,
  required MeshTransport transport,
  required String name,
  required int port,
  required List<String> peers,
  required bool multicast,
  required bool broadcast,
}) =>
    {
      'home': home,
      'transport': switch (transport) {
        MeshTransport.bluetooth => 'ble',
        MeshTransport.wifi => 'udp',
        MeshTransport.all => 'all',
      },
      'name': name,
      'port': port,
      'peers': peers,
      'multicast': multicast,
      'broadcast': broadcast,
    };

/// Which radios the mesh is using.
///
/// Phase 2D removed the choice: a device starts every radio it has. This enum survives
/// only so the CLI and the tests can still pin one, and so the UI can say which are up.
enum MeshTransport {
  /// UDP over Wi-Fi. Works laptop-to-laptop and phone-to-phone, but needs a shared
  /// network or hotspot.
  wifi,

  /// Bluetooth Low Energy via the native plugin. Needs no infrastructure at all.
  bluetooth,

  /// Everything this device has. The default, and the only value the app passes.
  all,
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
  String? _radioNotice;
  String _radioState = 'unknown';
  String _cadence = 'low_latency';

  /// Where the core keeps its state. Cached from the first [init] so [addPeer] can write
  /// beside it, and so a restart through [switchTransport] lands in the same directory
  /// rather than jumping to the real app-support path a test was avoiding.
  String? _homePath;
  List<String> _seedPeers = const [];
  bool _multicast = true;
  bool _broadcast = true;

  /// The unit the reporter last used. Kept here rather than in the widget so it survives
  /// tab changes and rebuilds - nobody picks their unit twice in an emergency.
  ///
  /// Session-scoped, not persisted to disk: that would need a preferences store the app
  /// does not carry yet, and the win from surviving a full restart is much smaller than
  /// the win from surviving a tab switch.
  RadiusUnit lastRadiusUnit = RadiusUnit.metres;

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

  /// The `host:port` addresses this node dials directly, in the order it will try them.
  ///
  /// On iOS this is the entire Wi-Fi reach of the node - see [udpAutoDiscoveryDefault] -
  /// so an empty list there is a fact worth putting on screen. Everywhere else it is
  /// extra reach on top of discovery, and empty is the normal state.
  List<String> get seedPeers => List.unmodifiable(_seedPeers);

  /// True when the node asked the UDP transport for the automatic discovery paths.
  bool get udpAutoDiscovery => _multicast || _broadcast;

  /// True when Wi-Fi is one of the radios currently carrying the mesh.
  bool get usingWifi => _transport != MeshTransport.bluetooth;

  /// True when Bluetooth is one of the radios currently carrying the mesh.
  bool get usingBluetooth =>
      (_transport == MeshTransport.bluetooth || _transport == MeshTransport.all) &&
      _bleError == null;

  /// The radios in use, in plain words, for anywhere that has to name them.
  String get radioNames => switch (_transport) {
        MeshTransport.bluetooth => 'Bluetooth',
        MeshTransport.wifi => 'Wi-Fi',
        MeshTransport.all => _bleError == null ? 'Bluetooth and Wi-Fi' : 'Wi-Fi',
      };

  /// Peers the radio can actually reach right now. Zero on Bluetooth means nothing is
  /// connected yet, which is the single most useful thing to show while someone waits.
  int get bleConnected => _bleConnected;
  String? get bleError => _bleError;

  /// The last state the platform reported for the radio: `on`, `off`, `unauthorized`,
  /// `unsupported`, `resetting`, or `unknown` before it has said anything.
  String get radioState => _radioState;

  /// How hard the radio is currently listening: `low_latency`, `balanced` or
  /// `low_power`. Shown so a quiet mesh does not look like a broken one.
  String get cadence => _cadence;

  /// Set when the mesh came up on a different radio than was asked for - a downgrade the
  /// user is entitled to know about, but which is not an error and must not read as one.
  String? get radioNotice => _radioNotice;
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

  Timer? _autoGpsTimer;

  /// Zero-config onboarding (plan.md §2): generate an identity, join `[default]`, done.
  /// No account, no sign-up, and nothing here touches the internet.
  /// [homeOverride], [port], [multicast] and [broadcast] exist so tests can start a real
  /// node without colliding with a node already running on this machine.
  ///
  /// [peers] are extra `host:port` addresses to dial directly, from the build's
  /// `--dart-define=MESH_PEERS=...`. Anything saved on the device by [addPeer] is merged
  /// in behind them. [multicast] and [broadcast] default per platform - see
  /// [udpAutoDiscoveryDefault], which is why they are nullable rather than `true`.
  Future<void> init({
    List<String> peers = const [],
    String? homeOverride,
    int port = 47474,
    bool? multicast,
    bool? broadcast,
    String? name,
    MeshTransport transport = MeshTransport.all,
  }) async {
    if (_started) return;
    _transport = transport;
    try {
      // A missing core is the one failure that has to name itself. Every call would
      // otherwise return an empty reply and the app would blame the mesh for a build step
      // nobody ran.
      if (_ffi.isStub) {
        _startError = MeshFfi.loadError ?? 'the mesh core could not be loaded';
        notifyListeners();
        return;
      }
      // Every radio this device has, started together. Only platforms with a native
      // Bluetooth layer get that one; the rest mesh over Wi-Fi alone and say so.
      //
      // Nothing here is fatal. A missing or refused radio costs that radio, never the
      // node: a phone with Bluetooth off still meshes over Wi-Fi, a laptop with no
      // peripheral role still meshes over UDP. Refusing to start turns "this device has
      // one radio" into "The mesh core did not start", which reads as a broken build.
      final wantBle =
          transport != MeshTransport.wifi && BleRadio.isAvailable;
      if (transport == MeshTransport.all && !BleRadio.isAvailable) {
        _radioNotice = 'No Bluetooth mesh on this platform - running over Wi-Fi. '
            'Other devices must be on the same Wi-Fi or hotspot.';
      }
      if (wantBle) {
        // Asked for when the radio needs it, not up front: an app that demands Bluetooth
        // before showing anything is an app people deny and then uninstall.
        final denied = await _requestBluetoothPermissions();
        if (denied != null) {
          // Degrade, do not stop. The mesh still has Wi-Fi, and the banner says why
          // Bluetooth is missing.
          _radioNotice = denied;
          _bleError = denied;
        }
      }
      _transport = wantBle && _bleError == null
          ? (transport == MeshTransport.all ? MeshTransport.all : MeshTransport.bluetooth)
          : MeshTransport.wifi;
      final homePath = await _resolveHome(homeOverride);

      // The build's peers first, then anything typed into the Radio panel on an earlier
      // run. Both routes exist because the compile-time one is all there is before the
      // app has ever been on the phone, and it is gone the moment somebody else builds.
      final saved = _readStoredPeers(homePath);
      _seedPeers = [...peers, ...saved.where((p) => !peers.contains(p))];

      final auto = udpAutoDiscoveryDefault(isIOS: Platform.isIOS);
      _multicast = multicast ?? auto;
      _broadcast = broadcast ?? auto;

      final reply = _ffi.start(startConfigJson(
        home: homePath,
        transport: _transport,
        name: name ?? _defaultName(),
        port: port,
        peers: _seedPeers,
        multicast: _multicast,
        broadcast: _broadcast,
      ));

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
      _autoGpsTimer =
          Timer.periodic(const Duration(minutes: 2), (_) => _autoShareLocation());

      if (wantBle && _bleError == null) await _startBluetooth();
      refresh();
      _autoShareLocation();
    } catch (e) {
      _startError = '$e';
    }
    notifyListeners();
  }

  /// Publish our own position periodically, so peers can place us and so a ghost has a
  /// recent last-known fix.
  ///
  /// This deliberately does **not** file a safety report. It used to: every two minutes
  /// it called `reportZone(..., 4)` - "this place is maximally safe" - from a GPS fix
  /// with no human involved. Under a safe/unsafe model that is a machine casting a
  /// safety vote about ground nobody looked at, which manufactures exactly the false
  /// consensus the vote counts exist to prevent. A safety claim needs a person behind it.
  Future<void> _autoShareLocation() async {
    if (!_started) return;
    final fix = await _currentFix();
    if (fix == null) return;
    _call({'cmd': 'set_location', 'lat': fix.$1, 'lon': fix.$2});
    _call({'cmd': 'share_location'});
    refresh();
  }

  // ----------------------------------------------------------------- seed peers

  /// The directory the core keeps its identity, contacts, networks and zones in, created
  /// if it is not there yet.
  Future<String> _resolveHome([String? override]) async {
    final cached = _homePath;
    if (override == null && cached != null) return cached;
    final path =
        override ?? '${(await getApplicationSupportDirectory()).path}/reunite';
    final dir = Directory(path);
    if (!dir.existsSync()) dir.createSync(recursive: true);
    _homePath = path;
    return path;
  }

  /// Peers the user typed in, one `host:port` per line.
  ///
  /// A plain text file beside the core's own state rather than a preferences plugin:
  /// `shared_preferences` would be a new dependency, and an offline-first app taking on
  /// another package to store one line is a bad trade.
  File _peersFile(String home) => File('$home/peers.txt');

  List<String> _readStoredPeers(String home) {
    try {
      final file = _peersFile(home);
      if (!file.existsSync()) return const [];
      return parsePeerList(file.readAsStringSync());
    } catch (e) {
      // A peer list we cannot read costs the peers, never the node.
      debugPrint('could not read saved peers: $e');
      return const [];
    }
  }

  /// Save a `host:port` address for this node to dial directly. Returns an error string,
  /// or null when it was saved.
  ///
  /// It reaches the transport at the **next start**, not now: seeds are handed to the UDP
  /// socket when it binds. The UI says so rather than implying the peer is already live.
  Future<String?> addPeer(String address) async {
    final value = address.trim();
    final err = peerAddressError(value);
    if (err != null) return err;
    if (_seedPeers.contains(value)) return null;
    return _writePeers([..._seedPeers, value]);
  }

  /// Forget a saved peer. A typo nobody can delete is a trap, so this exists.
  Future<String?> removePeer(String address) =>
      _writePeers(_seedPeers.where((p) => p != address).toList());

  Future<String?> _writePeers(List<String> peers) async {
    try {
      final home = await _resolveHome();
      _peersFile(home).writeAsStringSync(peers.isEmpty ? '' : '${peers.join('\n')}\n');
      _seedPeers = peers;
      notifyListeners();
      return null;
    } catch (e) {
      return 'could not save the peer list: $e';
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

    // Subscribe *before* starting, so the first state the platform reports is not lost.
    _bleEvents = _ble.events().listen((event) {
      switch (event['type'] as String?) {
        case 'frame':
          _ffi.bleInject(event['frame'] as String, event['from'] as String);
          break;
        case 'peer_lost':
          _ffi.blePeerLost(event['device'] as String);
          break;
        case 'rssi':
          _ffi.bleRssi(event['device'] as String, event['rssi'] as int);
          break;
        case 'radio_state':
          _onRadioState(event['state'] as String);
          break;
        case 'log':
          debugPrint('BLE: ${event['message']}');
          break;
      }
    }, onError: (Object e) => debugPrint('BLE event stream error: $e'));

    // Start unconditionally. The old code asked `isEnabled()` first and bailed out when
    // it said no - but on iOS that question has no answer until CoreBluetooth has
    // reported in, which it only does *after* a manager exists, which only `start()`
    // creates. It therefore always said no, and the iPhone never advertised or scanned
    // once. Power state now arrives asynchronously through `radio_state`, which is the
    // shape CoreBluetooth actually has.
    final err = await _ble.start();
    if (err != null) {
      _bleError = err;
      return;
    }
    _bleError = null;

    // ...and frames the core wants sent go out to the radio. 100ms keeps beacons
    // punctual without waking the radio pointlessly.
    _bleTimer = Timer.periodic(const Duration(milliseconds: 100), (_) => _pumpBluetooth());
  }

  /// Act on a radio state the platform reported.
  ///
  /// Only these strings may produce a "Bluetooth is off" message. A diagnostic that
  /// fires when the code does not actually know is worse than none: it sends someone to
  /// check a setting that was already correct, and they believe it.
  void _onRadioState(String state) {
    _radioState = state;
    if (state == 'on') {
      _bleError = null;
    } else {
      // Leaves an existing error alone for 'unknown' and 'resetting': the platform is
      // not claiming anything, so neither do we.
      _bleError = bleErrorForRadioState(state) ?? _bleError;
    }
    notifyListeners();
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
        final verdict = e['verdict'] as String;
        _add(
          verdict == 'safe' ? ChatKind.notice : ChatKind.warning,
          e['from'] as String,
          'zone now reads $verdict within ${formatRadius(e['radius_m'] as int)} '
          '(${e['safe_votes']} safe / ${e['unsafe_votes']} unsafe)',
        );
        break;
      case 'cadence':
        // The core decided the radio should ease off (or wake up). Pushing it down to
        // the platform is what turns the ladder into actual battery: the scanner is the
        // expensive half, and only Kotlin and Swift can reconfigure it.
        _ble.setCadence(
          scan: e['scan'] as String,
          windowMs: e['window_ms'] as int?,
          periodMs: e['period_ms'] as int?,
        );
        _cadence = e['scan'] as String;
        notifyListeners();
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

  /// File a safe/unsafe report about the area around a point.
  ///
  /// [radiusM] is already in metres: the unit picker lives in the UI and converts, so
  /// there is exactly one length unit below this line and no chance of a mixed-unit bug.
  String? reportZone(double lat, double lon, bool safe, int radiusM) {
    if (radiusM < kMinRadiusM || radiusM > kMaxRadiusM) {
      return 'radius must be between $kMinRadiusM m and ${kMaxRadiusM ~/ 1000} km';
    }
    final err = _ok(_call({
      'cmd': 'report_zone',
      'lat': lat,
      'lon': lon,
      'verdict': safe ? 'safe' : 'unsafe',
      'radius_m': radiusM,
    }));
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

  /// Report the area around wherever we are standing.
  Future<String?> reportZoneHere(bool safe, int radiusM) async {
    final fix = await _currentFix();
    if (fix == null) return 'no GPS fix - cannot report a zone without a position';
    return reportZone(fix.$1, fix.$2, safe, radiusM);
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
