library;

/// Plain Dart mirrors of the JSON the Rust core sends over the FFI bridge.
///
/// The field names match `crates/meshffi/src/dto.rs` exactly. Nothing here computes
/// anything about the mesh - ranking, distance, consensus and ghost detection all happen
/// in Rust, and this layer only carries the answers to the widgets.

class Peer {
  final String id;
  final String display;
  final bool direct;
  final int? hops;
  final int? rttMs;
  final int? rssi;
  final double? distanceM;
  final double? lat;
  final double? lon;
  final int lastSeenMs;
  final bool inCurrentNetwork;
  final int? battery;
  final int? status;
  final bool sos;

  /// Unreachable, but we still hold their last position. plan.md §3.2: a node whose
  /// battery died must not silently vanish from the map.
  final bool ghost;

  Peer.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        display = j['display'] as String,
        direct = j['direct'] as bool,
        hops = j['hops'] as int?,
        rttMs = j['rtt_ms'] as int?,
        rssi = j['rssi'] as int?,
        distanceM = (j['distance_m'] as num?)?.toDouble(),
        lat = (j['lat'] as num?)?.toDouble(),
        lon = (j['lon'] as num?)?.toDouble(),
        lastSeenMs = j['last_seen_ms'] as int,
        inCurrentNetwork = j['in_current_network'] as bool,
        battery = j['battery'] as int?,
        status = j['status'] as int?,
        sos = j['sos'] as bool,
        ghost = j['ghost'] as bool;

  bool get hasPosition => lat != null && lon != null;
}

/// One aggregated cell: is the area around it safe, how far does that reach, and how
/// many people are on each side of the question.
class Zone {
  final String cell;
  final double lat;
  final double lon;

  /// "safe" or "unsafe". Ties resolve to unsafe in the core, not here.
  final String verdict;

  /// Metres. The map draws a circle of this radius centred on (lat, lon).
  final int radiusM;

  /// Kept as two numbers, never one. plan.md §3.2 requires the count be visible next to
  /// the colour; folding them into a single score is what hid disagreement before.
  final int safeVotes;
  final int unsafeVotes;
  final int ageMs;
  final bool mine;

  Zone.fromJson(Map<String, dynamic> j)
      : cell = j['cell'] as String,
        lat = (j['lat'] as num).toDouble(),
        lon = (j['lon'] as num).toDouble(),
        verdict = j['verdict'] as String,
        radiusM = j['radius_m'] as int,
        safeVotes = j['safe_votes'] as int,
        unsafeVotes = j['unsafe_votes'] as int,
        ageMs = j['age_ms'] as int,
        mine = j['mine'] as bool;

  bool get isSafe => verdict == 'safe';
  int get totalVotes => safeVotes + unsafeVotes;

  /// True when people are actively reporting this cell both ways. Worth surfacing on its
  /// own: a contested area is a different thing from an unverified one.
  bool get contested => safeVotes > 0 && unsafeVotes > 0;
  bool get verified => totalVotes > 1;
}

/// The length units the reporter can type in. Feet and miles are here because the people
/// who need this app are not all on the metric system.
enum RadiusUnit {
  metres('m', 'metres', 1.0),
  kilometres('km', 'kilometres', 1000.0),
  feet('ft', 'feet', 0.3048),
  miles('mi', 'miles', 1609.344);

  final String short;
  final String label;
  final double inMetres;
  const RadiusUnit(this.short, this.label, this.inMetres);

  int toMetres(double length) => (length * inMetres).round();
}

/// Mirrors `zones::MIN_RADIUS_M` / `MAX_RADIUS_M`. Duplicated deliberately so the field
/// can reject a bad number before a round trip; the core validates again regardless.
const int kMinRadiusM = 10;
const int kMaxRadiusM = 20000;

class NetworkInfo {
  final String id;
  final String name;
  final List<String> members;
  final int memberCount;
  final int epoch;
  final bool storeMessages;
  final bool active;
  final bool isDefault;

  NetworkInfo.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        members = (j['members'] as List).cast<String>(),
        memberCount = j['member_count'] as int,
        epoch = j['epoch'] as int,
        storeMessages = j['store_messages'] as bool,
        active = j['active'] as bool,
        isDefault = j['is_default'] as bool;
}

class Whoami {
  final String id;
  final String? name;
  final String home;
  final String transport;
  final String network;
  final double? lat;
  final double? lon;
  final bool sos;
  final int? status;
  final int? battery;
  final int zoneResolution;

  Whoami.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String?,
        home = j['home'] as String,
        transport = j['transport'] as String,
        network = j['network'] as String,
        lat = (j['lat'] as num?)?.toDouble(),
        lon = (j['lon'] as num?)?.toDouble(),
        sos = j['sos'] as bool,
        status = j['status'] as int?,
        battery = j['battery'] as int?,
        zoneResolution = j['zone_resolution'] as int;
}

/// One of the pre-canned panic codes, read from the Rust core so the buttons cannot
/// drift out of sync with the protocol.
class StatusCode {
  final int code;
  final String name;
  final String text;
  StatusCode.fromJson(Map<String, dynamic> j)
      : code = j['code'] as int,
        name = j['name'] as String,
        text = j['text'] as String;
}

enum ChatKind { chat, direct, mine, notice, warning, sos, status }

class ChatMessage {
  final ChatKind kind;
  final String from;
  final String? fromId;
  final String text;
  final String network;
  final int? hops;
  final DateTime at;

  ChatMessage({
    required this.kind,
    required this.from,
    this.fromId,
    required this.text,
    this.network = '',
    this.hops,
    DateTime? at,
  }) : at = at ?? DateTime.now();

  bool get isMine => kind == ChatKind.mine;
}

/// Human-readable radius, mirroring `zones::fmt_radius` in the core.
String formatRadius(int metres) =>
    metres >= 1000 ? '${(metres / 1000).toStringAsFixed(1)} km' : '$metres m';
