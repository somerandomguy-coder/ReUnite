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

class Zone {
  final String cell;
  final double lat;
  final double lon;
  final int level;

  /// The 0..=4 user scale, fractional so a mixed average is visible.
  final double levelScaled;

  /// How many distinct nodes verified this cell. plan.md §3.2 requires this be shown
  /// alongside the colour, never folded into it.
  final int consensus;
  final int ageMs;
  final bool mine;

  Zone.fromJson(Map<String, dynamic> j)
      : cell = j['cell'] as String,
        lat = (j['lat'] as num).toDouble(),
        lon = (j['lon'] as num).toDouble(),
        level = j['level'] as int,
        levelScaled = (j['level_scaled'] as num).toDouble(),
        consensus = j['consensus'] as int,
        ageMs = j['age_ms'] as int,
        mine = j['mine'] as bool;

  bool get verified => consensus > 1;
}

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
