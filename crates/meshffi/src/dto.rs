//! The JSON contract between the Rust core and the Flutter UI.
//!
//! These mirror types exist so the wire format the UI sees is *stable and readable* -
//! node ids as hex strings, not byte arrays - and so the internal `meshcore` types stay
//! free to change without breaking Dart. Everything crossing the FFI boundary is one of
//! these, serialised as JSON.

use meshcore::node::{
    Event, NetworkView, PeerView, Reply, RouteView, WhoamiView,
};
use meshcore::store::StoredMessage;
use meshcore::zones::ZoneView;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct PeerDto {
    pub id: String,
    pub display: String,
    pub direct: bool,
    pub hops: Option<u8>,
    pub rtt_ms: Option<u64>,
    pub rssi: Option<i16>,
    pub distance_m: Option<f64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub last_seen_ms: u64,
    pub in_current_network: bool,
    pub battery: Option<u8>,
    pub status: Option<u8>,
    pub sos: bool,
    pub ghost: bool,
}

impl From<&PeerView> for PeerDto {
    fn from(p: &PeerView) -> Self {
        Self {
            id: p.id.to_hex(),
            display: p.display.clone(),
            direct: p.direct,
            hops: p.hops,
            rtt_ms: p.rtt_ms,
            rssi: p.rssi,
            distance_m: p.distance_m,
            lat: p.gps.map(|g| g.lat),
            lon: p.gps.map(|g| g.lon),
            last_seen_ms: p.last_seen_ms,
            in_current_network: p.in_current_network,
            battery: p.battery,
            status: p.status,
            sos: p.sos,
            ghost: p.ghost,
        }
    }
}

#[derive(Serialize)]
pub struct NetworkDto {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub member_count: usize,
    pub epoch: u32,
    pub store_messages: bool,
    pub active: bool,
    pub is_default: bool,
}

impl From<&NetworkView> for NetworkDto {
    fn from(n: &NetworkView) -> Self {
        Self {
            id: n.id.to_hex(),
            name: n.name.clone(),
            members: n.members.clone(),
            member_count: n.member_count,
            epoch: n.epoch,
            store_messages: n.store_messages,
            active: n.active,
            is_default: n.is_default,
        }
    }
}

#[derive(Serialize)]
pub struct RouteDto {
    pub dest: String,
    pub display: String,
    pub next_hop: String,
    pub next_hop_display: String,
    pub hops: u8,
    pub age_ms: u64,
}

impl From<&RouteView> for RouteDto {
    fn from(r: &RouteView) -> Self {
        Self {
            dest: r.dest.to_hex(),
            display: r.display.clone(),
            next_hop: r.next_hop.to_hex(),
            next_hop_display: r.next_hop_display.clone(),
            hops: r.hops,
            age_ms: r.age_ms,
        }
    }
}

#[derive(Serialize)]
pub struct WhoamiDto {
    pub id: String,
    pub name: Option<String>,
    pub home: String,
    pub transport: String,
    pub network: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub sos: bool,
    pub status: Option<u8>,
    pub battery: Option<u8>,
    pub zone_resolution: u8,
}

impl From<&WhoamiView> for WhoamiDto {
    fn from(w: &WhoamiView) -> Self {
        Self {
            id: w.id.to_hex(),
            name: w.name.clone(),
            home: w.home.clone(),
            transport: w.transport.clone(),
            network: w.network.clone(),
            lat: w.location.map(|g| g.lat),
            lon: w.location.map(|g| g.lon),
            sos: w.sos,
            status: w.status,
            battery: w.battery,
            zone_resolution: w.zone_resolution,
        }
    }
}

#[derive(Serialize)]
pub struct ZoneDto {
    pub cell: String,
    pub lat: f64,
    pub lon: f64,
    /// "safe" or "unsafe" - the aggregate call, ties resolving to unsafe.
    pub verdict: String,
    /// Metres. The map draws a circle of this radius at (lat, lon).
    pub radius_m: u32,
    /// Kept apart on purpose (plan.md §3.2): the UI must be able to show that people
    /// disagree, which a single blended number cannot express.
    pub safe_votes: u32,
    pub unsafe_votes: u32,
    pub age_ms: u64,
    pub mine: bool,
}

impl From<&ZoneView> for ZoneDto {
    fn from(z: &ZoneView) -> Self {
        Self {
            cell: format!("{:x}", z.cell),
            lat: z.lat,
            lon: z.lon,
            verdict: z.verdict.as_str().to_string(),
            radius_m: z.radius_m,
            safe_votes: z.safe_votes,
            unsafe_votes: z.unsafe_votes,
            age_ms: z.age_ms,
            mine: z.mine,
        }
    }
}

#[derive(Serialize)]
pub struct MessageDto {
    pub ts_ms: u64,
    pub network: String,
    pub network_name: String,
    pub kind: String,
    pub from: String,
    pub to: Option<String>,
    pub text: String,
}

impl From<&StoredMessage> for MessageDto {
    fn from(m: &StoredMessage) -> Self {
        Self {
            ts_ms: m.ts_ms,
            network: m.network.clone(),
            network_name: m.network_name.clone(),
            kind: m.kind.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            text: m.text.clone(),
        }
    }
}

/// Every reply the UI can receive, tagged so Dart can switch on `type`.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplyDto {
    Ok { message: String },
    Peers { peers: Vec<PeerDto> },
    Networks { networks: Vec<NetworkDto> },
    Routes { routes: Vec<RouteDto> },
    History { messages: Vec<MessageDto> },
    Whoami { whoami: WhoamiDto },
    Heatmap { zones: Vec<ZoneDto> },
    Error { message: String },
}

impl From<Reply> for ReplyDto {
    fn from(r: Reply) -> Self {
        match r {
            Reply::Ok(message) => ReplyDto::Ok { message },
            Reply::Peers(p) => ReplyDto::Peers {
                peers: p.iter().map(PeerDto::from).collect(),
            },
            Reply::Networks(n) => ReplyDto::Networks {
                networks: n.iter().map(NetworkDto::from).collect(),
            },
            Reply::Routes(r) => ReplyDto::Routes {
                routes: r.iter().map(RouteDto::from).collect(),
            },
            Reply::History(m) => ReplyDto::History {
                messages: m.iter().map(MessageDto::from).collect(),
            },
            Reply::Whoami(w) => ReplyDto::Whoami {
                whoami: WhoamiDto::from(&w),
            },
            Reply::Heatmap(z) => ReplyDto::Heatmap {
                zones: z.iter().map(ZoneDto::from).collect(),
            },
        }
    }
}

/// Every event the UI can receive, tagged so Dart can switch on `type`.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventDto {
    Chat {
        network: String,
        from_id: String,
        from: String,
        text: String,
        hops: u8,
    },
    Direct {
        network: String,
        from_id: String,
        from: String,
        text: String,
        hops: u8,
    },
    PeerJoined {
        id: String,
        display: String,
    },
    PeerLost {
        id: String,
        display: String,
    },
    LocationUpdate {
        id: String,
        display: String,
        lat: f64,
        lon: f64,
        distance_m: Option<f64>,
    },
    StatusUpdate {
        id: String,
        display: String,
        code: u8,
    },
    SosRaised {
        id: String,
        display: String,
        lat: Option<f64>,
        lon: Option<f64>,
        distance_m: Option<f64>,
    },
    SosCleared {
        id: String,
        display: String,
    },
    ZoneUpdate {
        cell: String,
        verdict: String,
        radius_m: u32,
        safe_votes: u32,
        unsafe_votes: u32,
        from: String,
    },
    Delivered {
        to: String,
        preview: String,
    },
    Cadence {
        hello_ms: u64,
        scan: String,
        window_ms: Option<u64>,
        period_ms: Option<u64>,
    },
    Context {
        network: String,
    },
    Notice {
        text: String,
    },
    Warning {
        text: String,
    },
}

impl From<Event> for EventDto {
    fn from(e: Event) -> Self {
        match e {
            Event::Chat { network, from_id, from, text, hops } => EventDto::Chat {
                network,
                from_id: from_id.to_hex(),
                from,
                text,
                hops,
            },
            Event::Direct { network, from_id, from, text, hops } => EventDto::Direct {
                network,
                from_id: from_id.to_hex(),
                from,
                text,
                hops,
            },
            Event::PeerJoined { id, display } => EventDto::PeerJoined {
                id: id.to_hex(),
                display,
            },
            Event::PeerLost { id, display } => EventDto::PeerLost {
                id: id.to_hex(),
                display,
            },
            Event::LocationUpdate { id, display, gps, distance_m } => EventDto::LocationUpdate {
                id: id.to_hex(),
                display,
                lat: gps.lat,
                lon: gps.lon,
                distance_m,
            },
            Event::StatusUpdate { id, display, code } => EventDto::StatusUpdate {
                id: id.to_hex(),
                display,
                code,
            },
            Event::SosRaised { id, display, gps, distance_m } => EventDto::SosRaised {
                id: id.to_hex(),
                display,
                lat: gps.map(|g| g.lat),
                lon: gps.map(|g| g.lon),
                distance_m,
            },
            Event::SosCleared { id, display } => EventDto::SosCleared {
                id: id.to_hex(),
                display,
            },
            Event::ZoneUpdate {
                cell,
                verdict,
                radius_m,
                safe_votes,
                unsafe_votes,
                from,
            } => EventDto::ZoneUpdate {
                cell: format!("{cell:x}"),
                verdict: verdict.as_str().to_string(),
                radius_m,
                safe_votes,
                unsafe_votes,
                from,
            },
            Event::Delivered { to, preview } => EventDto::Delivered { to, preview },
            Event::Cadence {
                hello_ms,
                scan,
                scan_window_ms,
            } => EventDto::Cadence {
                hello_ms,
                scan: scan.to_string(),
                window_ms: scan_window_ms.map(|(w, _)| w),
                period_ms: scan_window_ms.map(|(_, p)| p),
            },
            Event::Context(network) => EventDto::Context { network },
            Event::Notice(text) => EventDto::Notice { text },
            Event::Warning(text) => EventDto::Warning { text },
        }
    }
}

/// Startup configuration handed in from Dart.
#[derive(Deserialize)]
pub struct StartConfig {
    pub home: String,
    /// `"udp"` (Wi-Fi) or `"ble"` (frames handed to the platform's Bluetooth layer).
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Explicit peers, e.g. `["192.168.1.42:47474"]`. The reliable path on iOS, where
    /// multicast needs an Apple-granted entitlement.
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(default = "default_true")]
    pub multicast: bool,
    #[serde(default = "default_true")]
    pub broadcast: bool,
    #[serde(default)]
    pub battery: Option<u8>,
}

fn default_port() -> u16 {
    47474
}
fn default_transport() -> String {
    "udp".to_string()
}
fn default_true() -> bool {
    true
}

/// The command envelope Dart sends. `args` is interpreted per `cmd`.
#[derive(Deserialize)]
pub struct CommandDto {
    pub cmd: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub on: Option<bool>,
    #[serde(default)]
    pub code: Option<u8>,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    /// "safe" or "unsafe".
    #[serde(default)]
    pub verdict: Option<String>,
    /// Already in metres. The UI owns the unit picker and converts before it calls in,
    /// so the core has exactly one length unit and no chance of a mixed-unit bug.
    #[serde(default)]
    pub radius_m: Option<u32>,
    #[serde(default)]
    pub limit: Option<usize>,
}
