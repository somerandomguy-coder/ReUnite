//! The node actor: one task owns all mesh state and drives the radio.
//!
//! The CLI (or, later, a Flutter UI over `uniffi`) talks to it through two channels:
//! `Command` in, `Event` out. Nothing else touches the state, so the same core runs
//! unchanged under any front end - that is the "extract core" prerequisite for Phase 2.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use tokio::sync::{mpsc, oneshot};

use crate::battery;
use crate::beacon;
use crate::crypto;
use crate::geo::haversine_m;
use crate::identity::Identity;
use crate::net::{default_network_id, NetworkBook, DEFAULT_NETWORK_NAME};
use crate::packet::{
    Body, Envelope, Frame, Hello, Invite, InvitePayload, NetPayload, Packet, DEFAULT_TTL,
};
use crate::router::Router;
use crate::store::{self, Contact, StoredMessage};
use crate::types::{now_ms, Gps, MsgId, NetworkId, NodeId};
use crate::transport::Transport;
use crate::duty::{self, Cadence};
use crate::zones::{self, ZoneBook, ZoneView};

const OUTBOX_RETRY_MS: u64 = 15_000;
const OUTBOX_EXPIRY_MS: u64 = 120_000;
/// SOS is the one packet class allowed to be noisy: it gets a longer TTL than anything
/// else so it crosses a mesh that ordinary chat would not (plan.md §4 step 1.4).
const SOS_TTL: u8 = 12;

// --------------------------------------------------------------------- config

pub struct NodeConfig {
    pub home: PathBuf,
    pub self_name: Option<String>,
    pub location: Option<Gps>,
    /// Simulated radio range: if non-empty, only these node ids are audible.
    pub link_filter: HashSet<NodeId>,
    pub hello_interval: Duration,
    pub ping_interval: Duration,
    pub maintenance_interval: Duration,
    /// Force the advertised battery level. Keeps demos and tests deterministic, and lets
    /// a laptop on mains power still show a number.
    pub battery_override: Option<u8>,
    /// H3 resolution used to aggregate safety reports.
    pub zone_resolution: u8,
}

impl NodeConfig {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            self_name: None,
            location: None,
            link_filter: HashSet::new(),
            hello_interval: Duration::from_secs(3),
            ping_interval: Duration::from_secs(10),
            maintenance_interval: Duration::from_secs(5),
            battery_override: None,
            zone_resolution: zones::DEFAULT_RESOLUTION,
        }
    }
}

// ------------------------------------------------------------ commands/events

#[derive(Clone, Debug)]
pub enum Command {
    Broadcast(String),
    Direct { target: String, text: String },
    CreateNetwork(String),
    Invite { network: String, user: String },
    SetStoring { network: String, on: bool },
    Kick(String),
    Rename { user: String, name: String },
    Switch(String),
    Peers,
    Networks,
    Routes,
    History(usize),
    Whoami,
    SetLocation { lat: f64, lon: f64 },
    ShareLocation,
    SetLinkFilter(Vec<String>),
    /// Raise or clear the in-network SOS. Never touches OS emergency services.
    Sos(bool),
    /// Broadcast a pre-canned panic message. `0` clears it.
    SetStatus { code: u8 },
    /// Submit a safety report; snapped to an H3 cell before it goes anywhere.
    ReportZone {
        lat: f64,
        lon: f64,
        verdict: zones::Verdict,
        radius_m: u32,
    },
    Heatmap,
}

#[derive(Clone, Debug)]
pub enum Reply {
    Ok(String),
    Peers(Vec<PeerView>),
    Networks(Vec<NetworkView>),
    Routes(Vec<RouteView>),
    History(Vec<StoredMessage>),
    Whoami(WhoamiView),
    Heatmap(Vec<ZoneView>),
}

#[derive(Clone, Debug)]
pub struct PeerView {
    pub id: NodeId,
    pub display: String,
    pub direct: bool,
    pub hops: Option<u8>,
    pub rtt_ms: Option<u64>,
    pub rssi: Option<i16>,
    pub distance_m: Option<f64>,
    pub gps: Option<Gps>,
    pub last_seen_ms: u64,
    pub in_current_network: bool,
    /// Charge 0..=100 as last advertised, `None` if they never said.
    pub battery: Option<u8>,
    /// Last pre-canned status code (`status.rs`).
    pub status: Option<u8>,
    pub sos: bool,
    /// No route and no direct link, but we still hold their last position and timestamp.
    /// plan.md §3.2: a node whose battery died must not silently vanish from the map.
    pub ghost: bool,
}

#[derive(Clone, Debug)]
pub struct NetworkView {
    pub id: NetworkId,
    pub name: String,
    pub members: Vec<String>,
    pub member_count: usize,
    pub epoch: u32,
    pub store_messages: bool,
    pub active: bool,
    pub is_default: bool,
}

#[derive(Clone, Debug)]
pub struct RouteView {
    pub dest: NodeId,
    pub display: String,
    pub next_hop: NodeId,
    pub next_hop_display: String,
    pub hops: u8,
    pub age_ms: u64,
}

#[derive(Clone, Debug)]
pub struct WhoamiView {
    pub id: NodeId,
    pub name: Option<String>,
    pub home: String,
    pub transport: String,
    pub network: String,
    pub location: Option<Gps>,
    pub link_filter: Vec<NodeId>,
    pub sos: bool,
    pub status: Option<u8>,
    pub battery: Option<u8>,
    pub zone_resolution: u8,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Message on a network everyone in it can read.
    Chat {
        network: String,
        from_id: NodeId,
        from: String,
        text: String,
        hops: u8,
    },
    /// Message addressed to us alone.
    Direct {
        network: String,
        from_id: NodeId,
        from: String,
        text: String,
        hops: u8,
    },
    PeerJoined {
        id: NodeId,
        display: String,
    },
    PeerLost {
        id: NodeId,
        display: String,
    },
    LocationUpdate {
        id: NodeId,
        display: String,
        gps: Gps,
        distance_m: Option<f64>,
    },
    Delivered {
        to: String,
        preview: String,
    },
    /// Someone published a pre-canned panic message.
    StatusUpdate {
        id: NodeId,
        display: String,
        code: u8,
    },
    /// In-network SOS raised. Local mesh only - never emergency services.
    SosRaised {
        id: NodeId,
        display: String,
        gps: Option<Gps>,
        distance_m: Option<f64>,
    },
    SosCleared {
        id: NodeId,
        display: String,
    },
    /// A zone cell changed its verdict or gained a voter.
    ZoneUpdate {
        cell: u64,
        verdict: zones::Verdict,
        radius_m: u32,
        /// Kept as two numbers, never one. plan.md §3.2: "safe, 5 people say so" and
        /// "safe, 5 say so and 4 disagree" are different claims and must read differently.
        safe_votes: u32,
        unsafe_votes: u32,
        from: String,
    },
    /// The radio should change how hard it is beaconing and scanning.
    ///
    /// Emitted only when the rung changes, not on every tick: the platform reconfigures
    /// a scanner by restarting it, which is not something to do every three seconds.
    Cadence {
        hello_ms: u64,
        scan: &'static str,
        /// `Some((window_ms, period_ms))` for a windowed scan, `None` for continuous.
        scan_window_ms: Option<(u64, u64)>,
    },
    /// The active network changed; the CLI repaints its prompt from this.
    Context(String),
    Notice(String),
    Warning(String),
}

// --------------------------------------------------------------------- handle

#[derive(Clone)]
pub struct NodeHandle {
    pub id: NodeId,
    tx: mpsc::Sender<(Command, oneshot::Sender<Result<Reply>>)>,
}

impl NodeHandle {
    pub async fn call(&self, cmd: Command) -> Result<Reply> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send((cmd, reply_tx))
            .await
            .map_err(|_| anyhow!("mesh node stopped"))?;
        reply_rx.await.map_err(|_| anyhow!("mesh node stopped"))?
    }
}

// ---------------------------------------------------------------------- state

struct Pending {
    /// Every packet id this message has been sent under, so any Ack matches.
    ids: Vec<MsgId>,
    dest: NodeId,
    body: Body,
    preview: String,
    created_ms: u64,
    last_try_ms: u64,
}

pub struct Node {
    identity: Identity,
    /// Distinguishes this process from another one sharing the same identity file.
    instance: u64,
    /// Rate limit for the "someone else is using this identity" warning.
    clone_warned_ms: u64,
    /// Rate limit for undecodable-frame warnings (usually a peer on an older build).
    frame_warned_ms: u64,
    home: PathBuf,
    transport: Arc<dyn Transport>,
    contacts: HashMap<NodeId, Contact>,
    contacts_dirty: bool,
    networks: NetworkBook,
    router: Router,
    current: NetworkId,
    self_name: Option<String>,
    location: Option<Gps>,
    outbox: Vec<Pending>,
    events: mpsc::Sender<Event>,
    /// Our own in-network SOS flag; mirrored into every Hello and Beacon.
    sos: bool,
    /// Our own pre-canned status code, `None` when cleared.
    status: Option<u8>,
    zones: ZoneBook,
    battery_override: Option<u8>,
    zone_resolution: u8,
    /// Beacon v1 sequence counter (`beacon.rs`), wraps at 255.
    beacon_seq: u8,
    /// Round-robin cursor over our own zone reports, for periodic re-gossip.
    zone_gossip_idx: usize,
    /// When we last heard anything at all from anyone. Drives the duty-cycle ladder.
    last_heard_ms: u64,
    /// The cadence currently in force, so the radio is only reconfigured on a change.
    cadence: duty::Cadence,
}

impl Node {
    /// Start the node task. Returns a handle for commands and a stream of events.
    pub fn spawn(
        config: NodeConfig,
        transport: Arc<dyn Transport>,
    ) -> Result<(NodeHandle, mpsc::Receiver<Event>)> {
        let identity = Identity::load_or_create(&config.home)?;
        let contacts = store::load_contacts(&config.home)?;
        let networks = NetworkBook::load(&config.home, identity.id)?;
        let zones = ZoneBook::load(&config.home, config.zone_resolution)?;
        let mut router = Router::new(identity.id);
        router.set_link_filter(config.link_filter.clone());

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(256);
        let handle = NodeHandle {
            id: identity.id,
            tx: cmd_tx,
        };

        let node = Node {
            identity,
            instance: u64::from_le_bytes(crypto::random_bytes::<8>()),
            clone_warned_ms: 0,
            frame_warned_ms: 0,
            home: config.home.clone(),
            transport,
            contacts,
            contacts_dirty: false,
            networks,
            router,
            current: default_network_id(),
            self_name: config.self_name.clone(),
            location: config.location,
            outbox: Vec::new(),
            events: event_tx,
            sos: false,
            status: None,
            zones,
            battery_override: config.battery_override,
            zone_resolution: config.zone_resolution,
            beacon_seq: 0,
            zone_gossip_idx: 0,
            // Starting "just heard something" gives the join race its full first minute
            // at the fast rate, rather than treating a cold start as solitude.
            last_heard_ms: now_ms(),
            cadence: duty::Cadence::ENGAGED,
        };

        tokio::spawn(node.run(
            cmd_rx,
            config.hello_interval,
            config.ping_interval,
            config.maintenance_interval,
        ));
        Ok((handle, event_rx))
    }

    async fn run(
        mut self,
        mut cmd_rx: mpsc::Receiver<(Command, oneshot::Sender<Result<Reply>>)>,
        hello_interval: Duration,
        ping_interval: Duration,
        maintenance_interval: Duration,
    ) {
        let mut hello = tokio::time::interval(hello_interval);
        let mut ping = tokio::time::interval(ping_interval);
        let mut maintenance = tokio::time::interval(maintenance_interval);
        let transport = self.transport.clone();
        // `hello_interval` from the config is the *engaged* rate; the ladder scales it
        // from there. A test that pins a fast interval keeps a fast interval, because
        // its nodes can hear each other and never leave the top rung.
        let base_hello = hello_interval;
        let mut beacon_seq: u64 = 0;

        loop {
            tokio::select! {
                incoming = transport.recv() => match incoming {
                    Ok((bytes, from)) => {
                        if let Err(e) = self.on_frame(&bytes, from).await {
                            self.warn_frame_drop(from, &e.to_string());
                        }
                    }
                    Err(e) => {
                        let _ = self.events.try_send(Event::Warning(format!("transport error: {e}")));
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                },
                Some((cmd, reply)) = cmd_rx.recv() => {
                    let result = self.on_command(cmd).await;
                    let _ = reply.send(result);
                }
                _ = hello.tick() => {
                    let _ = self.send_hello().await;
                    beacon_seq = beacon_seq.wrapping_add(1);
                    // Re-arm at whatever rate the current conditions call for. Tokio's
                    // interval period is fixed at construction, so a change of rung means
                    // a new interval rather than a mutated one.
                    let next = self.current_cadence(base_hello);
                    if next.hello != self.cadence.hello || next.scan != self.cadence.scan {
                        self.cadence = next;
                        hello = tokio::time::interval(duty::jitter(next.hello, beacon_seq));
                        hello.tick().await; // the first tick of a fresh interval is immediate
                        let _ = self.events.try_send(Event::Cadence {
                            hello_ms: next.hello.as_millis() as u64,
                            scan: next.scan.as_str(),
                            scan_window_ms: next
                                .scan_window
                                .map(|(w, p)| (w.as_millis() as u64, p.as_millis() as u64)),
                        });
                    }
                }
                _ = ping.tick() => { let _ = self.ping_neighbors().await; }
                _ = maintenance.tick() => { self.maintenance().await; }
                else => break,
            }
        }
    }

    // ------------------------------------------------------------- outbound

    fn make_packet(&self, dest: Option<NodeId>, body: Body, ttl: u8) -> Packet {
        let mut packet = Packet {
            id: MsgId::random(),
            origin: self.identity.id,
            dest,
            sent_at_ms: now_ms(),
            body,
            sig: Vec::new(),
            ttl,
            path: Vec::new(),
        };
        packet.sig = self.identity.sign(&packet.signing_bytes());
        packet
    }

    async fn emit_frame(&self, packet: &Packet, to: Option<SocketAddr>) -> Result<()> {
        let frame = Frame::new(self.identity.id, self.instance, packet.clone());
        let bytes = frame.encode()?;
        match to {
            Some(addr) => self.transport.send_to(&bytes, addr).await,
            None => self.transport.send_broadcast(&bytes).await,
        }
    }

    /// Send a packet we authored: unicast along a known route, flood otherwise.
    async fn dispatch(&mut self, packet: Packet) -> Result<()> {
        self.router.mark_seen(packet.id);
        let target = packet.dest.and_then(|d| self.router.next_hop_addr(&d));
        self.emit_frame(&packet, target).await
    }

    /// Relay someone else's packet one hop further (store-and-forward, plan.md 1.5).
    async fn forward(&mut self, mut packet: Packet) -> Result<()> {
        if packet.ttl == 0 {
            return Ok(());
        }
        packet.ttl -= 1;
        packet.path.push(self.identity.id);
        let target = packet.dest.and_then(|d| self.router.next_hop_addr(&d));
        self.emit_frame(&packet, target).await
    }

    async fn send_hello(&mut self) -> Result<()> {
        let body = Body::Hello(Hello {
            ed_pub: self.identity.ed_public(),
            x_pub: self.identity.x_public(),
            name: self.self_name.clone(),
            gps: self.location,
            battery: self.battery_percent(),
            sos: self.sos,
            status: self.status,
        });
        let packet = self.make_packet(None, body, DEFAULT_TTL);
        self.beacon_seq = self.beacon_seq.wrapping_add(1);
        self.dispatch(packet).await
    }

    /// Charge to advertise: the `--battery` override wins, else ask the platform.
    fn battery_percent(&self) -> Option<u8> {
        self.battery_override.or_else(battery::read_percent)
    }

    /// This node's presence as a Beacon v1 advertisement (`beacon.rs`).
    ///
    /// Phase 1 does not have a radio that can advertise - `plan.md` deviation D3 - so
    /// nothing calls this on the hot path yet. It exists, and is tested, because it is
    /// the exact payload the Phase 2 native BLE layer and the Phase 3 firmware emit, and
    /// building the format against the real node state now is what keeps it honest.
    pub fn presence_beacon(&self) -> beacon::Beacon {
        let mut flags = beacon::FLAG_RELAY;
        if self.sos {
            flags |= beacon::FLAG_SOS;
        }
        if self.location.is_some() {
            flags |= beacon::FLAG_GPS;
        }
        if self.status.is_some() {
            flags |= beacon::FLAG_STATUS;
        }
        beacon::Beacon {
            header: beacon::Header {
                flags,
                battery: self.battery_percent().unwrap_or(beacon::BATTERY_UNKNOWN),
                seq: self.beacon_seq,
            },
            body: beacon::Body::Presence(beacon::Presence {
                node: self.identity.id,
                lat_e7: self.location.map(|g| beacon::to_e7(g.lat)).unwrap_or(0),
                lon_e7: self.location.map(|g| beacon::to_e7(g.lon)).unwrap_or(0),
                status: self.status.unwrap_or(crate::status::NONE),
                hops: 0,
                ttl: DEFAULT_TTL,
            }),
        }
    }

    /// One aggregated safe-zone cell as a Beacon v1 advertisement. Same reasoning as
    /// `presence_beacon`: the format is built and tested here, the radio arrives in Phase 2.
    pub fn zone_beacon(&self, cell: u64) -> Option<beacon::Beacon> {
        let zone = self.zones.get(cell)?;
        Some(beacon::Beacon {
            header: beacon::Header {
                flags: beacon::FLAG_RELAY,
                battery: self.battery_percent().unwrap_or(beacon::BATTERY_UNKNOWN),
                seq: self.beacon_seq,
            },
            body: beacon::Body::Zone(beacon::Zone {
                origin: self.identity.id,
                cell,
                verdict: zone.verdict().to_wire(),
                consensus: zone.consensus(),
                // The advertisement gets two bytes for this, so a radius past 65 km
                // cannot be expressed - `MAX_RADIUS_M` is far below that already.
                radius_m: zone.radius_m().min(u16::MAX as u32) as u16,
            }),
        })
    }

    /// The cadence the current conditions call for, scaled from the configured base rate.
    fn current_cadence(&self, base_hello: Duration) -> Cadence {
        let peers = self.router.neighbors().count();
        // Any SOS we know about, ours or a peer's. Relaying someone else's emergency is
        // not the time to be economical either.
        let sos = self.sos || self.contacts.values().any(|c| c.sos);
        let mut cadence = duty::cadence(duty::Conditions {
            alone_for_ms: now_ms().saturating_sub(self.last_heard_ms),
            peers,
            sos,
            battery: self.battery_percent(),
        });
        // Scale the ladder against the configured base, so `--hello-interval` style
        // overrides and the fast intervals the tests use are still honoured.
        if base_hello != Cadence::ENGAGED.hello {
            let ratio = cadence.hello.as_millis() as f64 / Cadence::ENGAGED.hello.as_millis() as f64;
            cadence.hello = base_hello.mul_f64(ratio);
        }
        cadence
    }

    async fn ping_neighbors(&mut self) -> Result<()> {
        let neighbors: Vec<(NodeId, SocketAddr)> = self
            .router
            .neighbors()
            .map(|n| (n.id, n.addr))
            .collect();
        for (id, addr) in neighbors {
            let nonce = u64::from_le_bytes(crypto::random_bytes::<8>());
            // ttl 0: latency probes are link-local and never relayed.
            let packet = self.make_packet(Some(id), Body::Ping { nonce }, 0);
            self.router.mark_seen(packet.id);
            let _ = self.emit_frame(&packet, Some(addr)).await;
        }
        Ok(())
    }

    fn seal(&self, network: &NetworkId, payload: &NetPayload) -> Result<Body> {
        let net = self
            .networks
            .get(network)
            .ok_or_else(|| anyhow!("unknown network"))?;
        let plaintext = bincode::serialize(payload)?;
        let (nonce, ciphertext) = crypto::sym_encrypt(&net.key, &plaintext)?;
        Ok(Body::Envelope(Envelope {
            network: net.id,
            epoch: net.epoch,
            nonce,
            ciphertext,
        }))
    }

    async fn send_payload(
        &mut self,
        network: NetworkId,
        dest: Option<NodeId>,
        payload: NetPayload,
    ) -> Result<MsgId> {
        self.send_payload_ttl(network, dest, payload, DEFAULT_TTL).await
    }

    async fn send_payload_ttl(
        &mut self,
        network: NetworkId,
        dest: Option<NodeId>,
        payload: NetPayload,
        ttl: u8,
    ) -> Result<MsgId> {
        let body = self.seal(&network, &payload)?;
        let packet = self.make_packet(dest, body, ttl);
        let id = packet.id;
        self.dispatch(packet).await?;
        Ok(id)
    }

    // -------------------------------------------------------------- inbound

    async fn on_frame(&mut self, bytes: &[u8], from: SocketAddr) -> Result<()> {
        // Before the frame is parsed, let alone accepted. Anything at all on the air
        // means we are not alone, including a frame we cannot decrypt or a version we do
        // not understand - being slow to notice a rescuer costs more than a beacon does.
        self.last_heard_ms = now_ms();

        let frame = Frame::decode(bytes)?;
        if frame.link_from == self.identity.id {
            if frame.instance != self.instance {
                // Same node id, different process: two nodes are sharing one --home.
                self.warn_identity_clone();
            }
            return Ok(()); // our own multicast echo
        }
        if !self.router.accepts_link(&frame.link_from) {
            return Ok(()); // simulated out of radio range
        }
        self.router.note_neighbor(frame.link_from, from);
        // The scanner sees signal strength per advertisement, keyed by a device id; only
        // here, when a frame names its sender, can that reading be attached to a node.
        // This is where `Router::note_rssi` finally gets a source - it has existed and
        // returned nothing useful since Phase 1, because UDP has no per-peer RSSI.
        if let Some(rssi) = self.transport.rssi_for(&from) {
            self.router.note_rssi(&frame.link_from, rssi);
        }

        let packet = frame.packet;
        self.router
            .learn_route(packet.origin, frame.link_from, packet.hops());

        if !self.router.mark_seen(packet.id) {
            return Ok(()); // duplicate flood copy: route already learned, drop it
        }

        if packet.origin == self.identity.id {
            // We never authored this (our own packets are already in the dedupe cache),
            // yet it claims to be from us: a twin process relayed by a neighbour. Never
            // let it become a contact - a node must not list itself as its own peer.
            self.warn_identity_clone();
            return Ok(());
        }

        let addressed_to_me = packet.dest == Some(self.identity.id);
        let broadcast = packet.dest.is_none();
        if addressed_to_me || broadcast {
            self.deliver(&packet, from).await?;
        }
        if !addressed_to_me {
            self.forward(packet).await?;
        }
        Ok(())
    }

    fn verify_origin(&self, packet: &Packet) -> Result<()> {
        // A Hello carries its own key: that is how a node is introduced in the first place.
        if let Body::Hello(hello) = &packet.body {
            return crypto::verify(&hello.ed_pub, &packet.signing_bytes(), &packet.sig);
        }
        let contact = self
            .contacts
            .get(&packet.origin)
            .ok_or_else(|| anyhow!("no key yet for {}", packet.origin))?;
        crypto::verify(&contact.ed_pub, &packet.signing_bytes(), &packet.sig)
    }

    async fn deliver(&mut self, packet: &Packet, from: SocketAddr) -> Result<()> {
        if !matches!(packet.body, Body::Hello(_)) && !self.contacts.contains_key(&packet.origin) {
            // Their beacon has not reached us yet, so we cannot check the signature. It
            // arrives within a few seconds and they will retry; staying quiet beats
            // shouting about a normal startup race.
            return Ok(());
        }
        self.verify_origin(packet)?;
        match &packet.body {
            Body::Hello(hello) => self.on_hello(packet, hello).await,
            Body::Ping { nonce } => {
                let body = Body::Pong {
                    nonce: *nonce,
                    echo_sent_ms: packet.sent_at_ms,
                };
                let pong = self.make_packet(Some(packet.origin), body, 0);
                self.router.mark_seen(pong.id);
                self.emit_frame(&pong, Some(from)).await
            }
            Body::Pong { echo_sent_ms, .. } => {
                let rtt = now_ms().saturating_sub(*echo_sent_ms);
                self.router.note_rtt(&packet.origin, rtt);
                Ok(())
            }
            Body::Invite(invite) => self.on_invite(packet, invite).await,
            Body::Envelope(env) => self.on_envelope(packet, env.clone()).await,
        }
    }

    async fn on_hello(&mut self, packet: &Packet, hello: &Hello) -> Result<()> {
        let id = packet.origin;
        let is_new = !self.contacts.contains_key(&id);
        let entry = self.contacts.entry(id).or_insert_with(|| Contact {
            id,
            ed_pub: hello.ed_pub,
            x_pub: hello.x_pub,
            alias: None,
            self_name: None,
            last_seen_ms: 0,
            gps: None,
            battery: None,
            status: None,
            sos: false,
        });
        entry.ed_pub = hello.ed_pub;
        entry.x_pub = hello.x_pub;
        entry.self_name = hello.name.clone().or_else(|| entry.self_name.clone());
        entry.last_seen_ms = now_ms();
        let moved = match (entry.gps, hello.gps) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(old), Some(new)) => new.ts_ms > old.ts_ms && (old.lat, old.lon) != (new.lat, new.lon),
        };
        if let Some(gps) = hello.gps {
            entry.gps = Some(gps);
        }
        // A Hello is authoritative about its author's own state, so status and SOS are
        // taken as given - that is how clearing them propagates. Battery is kept on a
        // last-known basis because a platform that cannot read it sends None forever.
        let sos_before = entry.sos;
        let status_before = entry.status;
        entry.battery = hello.battery.or(entry.battery);
        entry.sos = hello.sos;
        entry.status = hello.status;
        let display = entry.display();
        self.contacts_dirty = true;

        // Everyone we can hear is a member of [default]; private networks are explicit.
        if let Some(def) = self.networks.get_mut(&default_network_id()) {
            def.members.insert(id);
        }

        if is_new {
            let _ = self.events.try_send(Event::PeerJoined { id, display: display.clone() });
        }
        // Beacons repeat every few seconds; only a *change* is worth telling anyone about.
        if hello.sos != sos_before {
            let _ = self.events.try_send(if hello.sos {
                Event::SosRaised {
                    id,
                    display: display.clone(),
                    gps: hello.gps,
                    distance_m: match (self.location, hello.gps) {
                        (Some(mine), Some(theirs)) => Some(haversine_m(&mine, &theirs)),
                        _ => None,
                    },
                }
            } else {
                Event::SosCleared {
                    id,
                    display: display.clone(),
                }
            });
        }
        if hello.status != status_before {
            if let Some(code) = hello.status {
                let _ = self.events.try_send(Event::StatusUpdate {
                    id,
                    display: display.clone(),
                    code,
                });
            }
        }
        if moved {
            if let Some(gps) = hello.gps {
                let distance_m = self.location.as_ref().map(|mine| haversine_m(mine, &gps));
                let _ = self.events.try_send(Event::LocationUpdate {
                    id,
                    display,
                    gps,
                    distance_m,
                });
            }
        }
        Ok(())
    }

    async fn on_invite(&mut self, packet: &Packet, invite: &Invite) -> Result<()> {
        let plaintext = crypto::open_sealed(&self.identity.exchange, &invite.sealed)?;
        let payload: InvitePayload = bincode::deserialize(&plaintext)?;
        if payload.network != invite.network {
            bail!("invite network mismatch");
        }
        let inviter = self.display_of(&packet.origin);
        let joined = self.networks.accept_invite(&payload)?;
        let msg = if joined {
            format!(
                "{inviter} added you to network '{}' ({} members). Use --switch {} to talk there.",
                payload.name,
                payload.members.len(),
                payload.name
            )
        } else {
            format!(
                "network '{}' re-keyed to epoch {} ({} members)",
                payload.name,
                payload.epoch,
                payload.members.len()
            )
        };
        let _ = self.events.try_send(Event::Notice(msg));
        Ok(())
    }

    async fn on_envelope(&mut self, packet: &Packet, env: Envelope) -> Result<()> {
        let Some(net) = self.networks.get(&env.network) else {
            return Ok(()); // not ours to read - we already relayed it
        };
        let Some(key) = net.key_for_epoch(env.epoch) else {
            return Ok(()); // older/newer key generation we do not hold
        };
        let network_id = net.id;
        let network_name = net.name.clone();
        let store_messages = net.store_messages;
        let plaintext = match crypto::sym_decrypt(&key, &env.nonce, &env.ciphertext) {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        let payload: NetPayload = bincode::deserialize(&plaintext)?;
        let from_id = packet.origin;
        let from = self.display_of(&from_id);

        match payload {
            NetPayload::Chat { text } => {
                if store_messages {
                    self.store_message(&network_id, &network_name, "chat", &from_id, None, &text)?;
                }
                let _ = self.events.try_send(Event::Chat {
                    network: network_name,
                    from_id,
                    from,
                    text,
                    hops: packet.hops(),
                });
            }
            NetPayload::Direct { text } => {
                if store_messages {
                    self.store_message(
                        &network_id,
                        &network_name,
                        "direct",
                        &from_id,
                        Some(&self.identity.id),
                        &text,
                    )?;
                }
                let _ = self.events.try_send(Event::Direct {
                    network: network_name,
                    from_id,
                    from,
                    text,
                    hops: packet.hops(),
                });
                let ack = NetPayload::Ack { msg: packet.id };
                let _ = self.send_payload(network_id, Some(from_id), ack).await;
            }
            NetPayload::Ack { msg } => {
                if let Some(pos) = self.outbox.iter().position(|p| p.ids.contains(&msg)) {
                    let pending = self.outbox.remove(pos);
                    let _ = self.events.try_send(Event::Delivered {
                        to: self.display_of(&pending.dest),
                        preview: pending.preview,
                    });
                }
            }
            NetPayload::Gps(gps) => {
                if let Some(c) = self.contacts.get_mut(&from_id) {
                    c.gps = Some(gps);
                    self.contacts_dirty = true;
                }
                let distance_m = self.location.as_ref().map(|mine| haversine_m(mine, &gps));
                let _ = self.events.try_send(Event::LocationUpdate {
                    id: from_id,
                    display: from,
                    gps,
                    distance_m,
                });
            }
            NetPayload::Members { members, epoch } => {
                if let Some(net) = self.networks.get_mut(&network_id) {
                    if epoch >= net.epoch && net.members.contains(&from_id) {
                        net.members = members.into_iter().collect();
                        self.networks.save()?;
                    }
                }
            }
            NetPayload::KickVote { target, epoch } => {
                self.on_kick_vote(network_id, from_id, target, epoch).await?;
            }
            NetPayload::Status { code } => {
                let changed = match self.contacts.get_mut(&from_id) {
                    Some(c) => {
                        let before = c.status;
                        c.status = if code == crate::status::NONE {
                            None
                        } else {
                            Some(code)
                        };
                        self.contacts_dirty = true;
                        before != c.status
                    }
                    None => true,
                };
                if changed {
                    if store_messages {
                        self.store_message(
                            &network_id,
                            &network_name,
                            "status",
                            &from_id,
                            None,
                            crate::status::describe(code),
                        )?;
                    }
                    let _ = self.events.try_send(Event::StatusUpdate {
                        id: from_id,
                        display: from,
                        code,
                    });
                }
            }
            NetPayload::Sos { active, gps } => {
                if let Some(c) = self.contacts.get_mut(&from_id) {
                    c.sos = active;
                    if let Some(g) = gps {
                        c.gps = Some(g);
                    }
                    self.contacts_dirty = true;
                }
                if store_messages {
                    self.store_message(
                        &network_id,
                        &network_name,
                        "sos",
                        &from_id,
                        None,
                        if active { "SOS raised" } else { "SOS cleared" },
                    )?;
                }
                let _ = self.events.try_send(if active {
                    Event::SosRaised {
                        id: from_id,
                        display: from,
                        gps,
                        distance_m: match (self.location, gps) {
                            (Some(mine), Some(theirs)) => Some(haversine_m(&mine, &theirs)),
                            _ => None,
                        },
                    }
                } else {
                    Event::SosCleared {
                        id: from_id,
                        display: from,
                    }
                });
            }
            NetPayload::Zone {
                cell,
                verdict,
                radius_m,
            } => {
                // The reporter is the packet origin, not whoever relayed it - that is what
                // keeps the vote counts one-per-person.
                let verdict = zones::Verdict::from_wire(verdict);
                if self
                    .zones
                    .record(cell, from_id, verdict, radius_m, packet.sent_at_ms)
                {
                    self.zones.save()?;
                    if let Some(zone) = self.zones.get(cell) {
                        let _ = self.events.try_send(Event::ZoneUpdate {
                            cell,
                            verdict: zone.verdict(),
                            radius_m: zone.radius_m(),
                            safe_votes: zone.safe_votes(),
                            unsafe_votes: zone.unsafe_votes(),
                            from,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Tally a ballot and, once the network agrees, rotate the key so the removed node
    /// can no longer read anything (plan.md §4 step 1.4).
    async fn on_kick_vote(
        &mut self,
        network_id: NetworkId,
        voter: NodeId,
        target: NodeId,
        epoch: u32,
    ) -> Result<()> {
        let (tally, threshold, name, reached) = {
            let Some(net) = self.networks.get_mut(&network_id) else {
                return Ok(());
            };
            if epoch != net.epoch || !net.members.contains(&voter) || !net.members.contains(&target)
            {
                return Ok(());
            }
            let voters = net.votes.entry((target, epoch)).or_default();
            voters.insert(voter);
            let tally = voters.len();
            let threshold = net.kick_threshold();
            (tally, threshold, net.name.clone(), tally >= threshold)
        };

        let target_name = self.display_of(&target);
        let voter_name = self.display_of(&voter);
        let _ = self.events.try_send(Event::Notice(format!(
            "[{name}] {voter_name} voted to kick {target_name} ({tally}/{threshold})"
        )));

        if !reached || target == self.identity.id {
            return Ok(());
        }

        let leader = self
            .networks
            .get(&network_id)
            .and_then(|n| n.rekey_leader(&target));
        if leader != Some(self.identity.id) {
            // Someone else mints the new key; we will get it as an Invite.
            return Ok(());
        }

        let new_epoch = self.networks.rekey(&network_id, &target)?;
        let members: Vec<NodeId> = self
            .networks
            .get(&network_id)
            .map(|n| n.members.iter().copied().collect())
            .unwrap_or_default();
        let me = self.identity.id;
        let recipients: Vec<NodeId> = members.iter().copied().filter(|m| *m != me).collect();
        for member in recipients {
            let _ = self.send_invite(network_id, member).await;
        }
        let _ = self
            .send_payload(
                network_id,
                None,
                NetPayload::Members {
                    members,
                    epoch: new_epoch,
                },
            )
            .await;
        let _ = self.events.try_send(Event::Notice(format!(
            "[{name}] {target_name} removed; network re-keyed to epoch {new_epoch}"
        )));
        Ok(())
    }

    async fn send_invite(&mut self, network_id: NetworkId, to: NodeId) -> Result<()> {
        let contact = self
            .contacts
            .get(&to)
            .ok_or_else(|| anyhow!("no public key for {to} yet - wait until they appear in --peers"))?;
        let x_pub = contact.x_pub;
        let net = self
            .networks
            .get(&network_id)
            .ok_or_else(|| anyhow!("unknown network"))?;
        let payload = InvitePayload {
            network: net.id,
            name: net.name.clone(),
            creator: net.creator,
            epoch: net.epoch,
            key: net.key,
            members: net.members.iter().copied().collect(),
        };
        let sealed = crypto::seal_to(&x_pub, &bincode::serialize(&payload)?)?;
        let body = Body::Invite(Invite {
            network: net.id,
            epoch: net.epoch,
            sealed,
        });
        let packet = self.make_packet(Some(to), body, DEFAULT_TTL);
        self.dispatch(packet).await
    }

    // ---------------------------------------------------------- housekeeping

    /// Undecodable frames are usually a peer running an older build. Say so once, not
    /// every three seconds, and say what to do about it.
    fn warn_frame_drop(&mut self, from: SocketAddr, error: &str) {
        let now = now_ms();
        if now.saturating_sub(self.frame_warned_ms) < 30_000 {
            return;
        }
        self.frame_warned_ms = now;
        let hint = if error.contains("protocol version") {
            " - rebuild every machine from the same commit so they speak the same protocol"
        } else {
            ""
        };
        let _ = self
            .events
            .try_send(Event::Warning(format!("ignoring frames from {from}: {error}{hint}")));
    }

    /// Tell the user, at most twice a minute, that another process owns this identity.
    fn warn_identity_clone(&mut self) {
        let now = now_ms();
        if now.saturating_sub(self.clone_warned_ms) < 30_000 {
            return;
        }
        self.clone_warned_ms = now;
        let _ = self.events.try_send(Event::Warning(format!(
            "another process is running this same identity ({}) from {} - two nodes \
             sharing one home directory are one node and cannot see each other. Give each \
             its own state and port, e.g. --home ./nodeB --port 47475",
            self.identity.id.to_hex(),
            self.home.display()
        )));
    }

    async fn maintenance(&mut self) {
        for id in self.router.prune() {
            let display = self.display_of(&id);
            let _ = self.events.try_send(Event::PeerLost { id, display });
        }

        if self.contacts_dirty {
            if let Err(e) = store::save_contacts(&self.home, &self.contacts) {
                let _ = self
                    .events
                    .try_send(Event::Warning(format!("could not save contacts: {e}")));
            }
            self.contacts_dirty = false;
        }

        self.gossip_zone().await;

        // A safe zone six hours old is not evidence about now.
        if self.zones.prune(now_ms()) > 0 {
            if let Err(e) = self.zones.save() {
                let _ = self
                    .events
                    .try_send(Event::Warning(format!("could not save zones: {e}")));
            }
        }

        // Retry undelivered direct messages: the route may exist now, or a relay that
        // was out of range may have wandered back in.
        let now = now_ms();
        let mut expired: Vec<Pending> = Vec::new();
        let mut retry: Vec<usize> = Vec::new();
        for (i, pending) in self.outbox.iter().enumerate() {
            if now.saturating_sub(pending.created_ms) > OUTBOX_EXPIRY_MS {
                continue;
            }
            if now.saturating_sub(pending.last_try_ms) >= OUTBOX_RETRY_MS {
                retry.push(i);
            }
        }
        for i in retry {
            let (dest, body) = {
                let p = &mut self.outbox[i];
                p.last_try_ms = now;
                (p.dest, p.body.clone())
            };
            let packet = self.make_packet(Some(dest), body, DEFAULT_TTL);
            let id = packet.id;
            self.outbox[i].ids.push(id);
            let _ = self.dispatch(packet).await;
        }
        let mut i = 0;
        while i < self.outbox.len() {
            if now.saturating_sub(self.outbox[i].created_ms) > OUTBOX_EXPIRY_MS {
                expired.push(self.outbox.remove(i));
            } else {
                i += 1;
            }
        }
        for pending in expired {
            let to = self.display_of(&pending.dest);
            let _ = self.events.try_send(Event::Warning(format!(
                "gave up delivering to {to}: \"{}\"",
                pending.preview
            )));
        }
    }

    /// Re-publish one of our own safety reports per maintenance tick, round-robin.
    ///
    /// A zone report is a single broadcast, so anyone who was still starting up - or who
    /// walked into range afterwards - would never learn it. Chat can be retried by a
    /// human and SOS/status ride every Hello, but the heat map had no such path. One
    /// cell per tick is bounded and cheap, and it converges a late joiner onto the whole
    /// map. It also keeps a live node's reports fresh against the TTL, while a node that
    /// has gone away stops refreshing and correctly ages out.
    async fn gossip_zone(&mut self) {
        let mine = self.zones.mine(&self.identity.id);
        if mine.is_empty() {
            return;
        }
        let (cell, verdict, radius_m) = mine[self.zone_gossip_idx % mine.len()];
        self.zone_gossip_idx = self.zone_gossip_idx.wrapping_add(1);
        let net = self.current;
        let _ = self
            .send_payload(
                net,
                None,
                NetPayload::Zone {
                    cell,
                    verdict: verdict.to_wire(),
                    radius_m,
                },
            )
            .await;
    }

    fn store_message(
        &self,
        network: &NetworkId,
        network_name: &str,
        kind: &str,
        from: &NodeId,
        to: Option<&NodeId>,
        text: &str,
    ) -> Result<()> {
        store::append_message(
            &self.home,
            network,
            &StoredMessage {
                ts_ms: now_ms(),
                network: network.to_hex(),
                network_name: network_name.to_string(),
                kind: kind.to_string(),
                from: from.to_hex(),
                to: to.map(|t| t.to_hex()),
                text: text.to_string(),
            },
        )
    }

    fn display_of(&self, id: &NodeId) -> String {
        if *id == self.identity.id {
            return self
                .self_name
                .clone()
                .unwrap_or_else(|| format!("{} (me)", id.to_hex()));
        }
        self.contacts
            .get(id)
            .map(|c| c.display())
            .unwrap_or_else(|| id.to_hex())
    }

    /// Accept a full id, a unique id prefix, a `--rename` alias, or a self-advertised name.
    fn resolve_node(&self, needle: &str) -> Result<NodeId> {
        let needle = needle.trim();
        if needle.is_empty() {
            bail!("expected a user id or name");
        }
        if let Ok(id) = NodeId::from_hex(needle) {
            return Ok(id);
        }
        let lower = needle.to_lowercase();
        let by_name: Vec<NodeId> = self
            .contacts
            .values()
            .filter(|c| {
                c.alias.as_deref().map(|a| a.to_lowercase()) == Some(lower.clone())
                    || c.self_name.as_deref().map(|n| n.to_lowercase()) == Some(lower.clone())
            })
            .map(|c| c.id)
            .collect();
        if by_name.len() == 1 {
            return Ok(by_name[0]);
        }
        if by_name.len() > 1 {
            bail!("'{needle}' matches {} peers - use the id", by_name.len());
        }
        let by_prefix: Vec<NodeId> = self
            .contacts
            .keys()
            .filter(|id| id.to_hex().starts_with(&lower))
            .copied()
            .collect();
        match by_prefix.len() {
            1 => Ok(by_prefix[0]),
            0 => Err(anyhow!("unknown user '{needle}' (see --peers)")),
            n => Err(anyhow!("'{needle}' matches {n} peers - type more of the id")),
        }
    }

    // -------------------------------------------------------------- commands

    async fn on_command(&mut self, cmd: Command) -> Result<Reply> {
        match cmd {
            Command::Broadcast(text) => self.cmd_broadcast(text).await,
            Command::Direct { target, text } => self.cmd_direct(target, text).await,
            Command::CreateNetwork(name) => self.cmd_create_network(name).await,
            Command::Invite { network, user } => self.cmd_invite(network, user).await,
            Command::SetStoring { network, on } => self.cmd_set_storing(network, on),
            Command::Kick(user) => self.cmd_kick(user).await,
            Command::Rename { user, name } => self.cmd_rename(user, name),
            Command::Switch(name) => self.cmd_switch(name),
            Command::Peers => Ok(Reply::Peers(self.peer_views())),
            Command::Networks => Ok(Reply::Networks(self.network_views())),
            Command::Routes => Ok(Reply::Routes(self.route_views())),
            Command::History(limit) => {
                let net = self.current_network_id();
                Ok(Reply::History(store::read_messages(&self.home, &net, limit)?))
            }
            Command::Whoami => Ok(Reply::Whoami(WhoamiView {
                id: self.identity.id,
                name: self.self_name.clone(),
                home: self.home.display().to_string(),
                transport: self.transport.describe(),
                network: self.current_network_name(),
                location: self.location,
                link_filter: self.router.link_filter().iter().copied().collect(),
                sos: self.sos,
                status: self.status,
                battery: self.battery_percent(),
                zone_resolution: self.zone_resolution,
            })),
            Command::SetLocation { lat, lon } => {
                self.location = Some(Gps {
                    lat,
                    lon,
                    ts_ms: now_ms(),
                });
                self.send_hello().await?;
                Ok(Reply::Ok(format!("location set to {lat:.5}, {lon:.5}")))
            }
            Command::ShareLocation => self.cmd_share_location().await,
            Command::SetLinkFilter(users) => self.cmd_link_filter(users),
            Command::Sos(active) => self.cmd_sos(active).await,
            Command::SetStatus { code } => self.cmd_status(code).await,
            Command::ReportZone {
                lat,
                lon,
                verdict,
                radius_m,
            } => self.cmd_report_zone(lat, lon, verdict, radius_m).await,
            Command::Heatmap => Ok(Reply::Heatmap(
                self.zones.views(&self.identity.id, now_ms()),
            )),
        }
    }

    fn current_network_id(&self) -> NetworkId {
        self.current
    }

    fn current_network_name(&self) -> String {
        self.networks
            .get(&self.current)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| DEFAULT_NETWORK_NAME.to_string())
    }

    async fn cmd_broadcast(&mut self, text: String) -> Result<Reply> {
        if text.trim().is_empty() {
            bail!("nothing to say");
        }
        let net = self.current;
        let (name, store_it) = {
            let n = self.networks.get(&net).ok_or_else(|| anyhow!("no network"))?;
            (n.name.clone(), n.store_messages)
        };
        self.send_payload(net, None, NetPayload::Chat { text: text.clone() })
            .await?;
        if store_it {
            self.store_message(&net, &name, "chat", &self.identity.id, None, &text)?;
        }
        Ok(Reply::Ok(format!("[{name}] sent to network")))
    }

    async fn cmd_direct(&mut self, target: String, text: String) -> Result<Reply> {
        let dest = self.resolve_node(&target)?;
        if dest == self.identity.id {
            bail!("that is you");
        }
        let net = self.current;
        let (name, store_it, is_member) = {
            let n = self.networks.get(&net).ok_or_else(|| anyhow!("no network"))?;
            (
                n.name.clone(),
                n.store_messages,
                n.is_default() || n.members.contains(&dest),
            )
        };
        if !is_member {
            bail!(
                "{} is not in [{name}] - invite them with --network {name} --add {}",
                self.display_of(&dest),
                dest
            );
        }
        let payload = NetPayload::Direct { text: text.clone() };
        let body = self.seal(&net, &payload)?;
        let packet = self.make_packet(Some(dest), body.clone(), DEFAULT_TTL);
        let id = packet.id;
        let routed = self.router.has_route(&dest);
        self.dispatch(packet).await?;
        self.outbox.push(Pending {
            ids: vec![id],
            dest,
            body,
            preview: text.chars().take(40).collect(),
            created_ms: now_ms(),
            last_try_ms: now_ms(),
        });
        if store_it {
            self.store_message(
                &net,
                &name,
                "direct",
                &self.identity.id,
                Some(&dest),
                &text,
            )?;
        }
        let how = if routed {
            match self.router.route(&dest) {
                Some(r) => format!("via {} ({} hops)", self.display_of(&r.next_hop), r.hops),
                None => "flooded".to_string(),
            }
        } else {
            "no route yet - flooding and retrying".to_string()
        };
        Ok(Reply::Ok(format!(
            "[{name}] -> {}: {how}",
            self.display_of(&dest)
        )))
    }

    async fn cmd_create_network(&mut self, name: String) -> Result<Reply> {
        let id = self.networks.create(&name, self.identity.id)?;
        self.current = id;
        let _ = self.events.try_send(Event::Context(name.clone()));
        Ok(Reply::Ok(format!(
            "created private network '{name}' ({id}) and switched to it - invite people with --network {name} --add [id]"
        )))
    }

    async fn cmd_invite(&mut self, network: String, user: String) -> Result<Reply> {
        let net_id = self.networks.resolve(&network)?;
        let target = self.resolve_node(&user)?;
        {
            let net = self.networks.get(&net_id).ok_or_else(|| anyhow!("no network"))?;
            if net.is_default() {
                bail!("[default] is open to everyone - nothing to invite to");
            }
            if !net.members.contains(&self.identity.id) {
                bail!("you are not a member of '{}'", net.name);
            }
        }
        if !self.contacts.contains_key(&target) {
            bail!("no public key for {target} yet - wait for them to appear in --peers");
        }
        let (name, members) = {
            let net = self.networks.get_mut(&net_id).unwrap();
            net.members.insert(target);
            (net.name.clone(), net.members.iter().copied().collect::<Vec<_>>())
        };
        self.networks.save()?;
        self.send_invite(net_id, target).await?;
        let epoch = self.networks.get(&net_id).map(|n| n.epoch).unwrap_or(0);
        let _ = self
            .send_payload(net_id, None, NetPayload::Members { members, epoch })
            .await;
        Ok(Reply::Ok(format!(
            "sealed the '{name}' key to {} and sent the invite",
            self.display_of(&target)
        )))
    }

    fn cmd_set_storing(&mut self, network: String, on: bool) -> Result<Reply> {
        let net_id = self.networks.resolve(&network)?;
        let name = {
            let net = self
                .networks
                .get_mut(&net_id)
                .ok_or_else(|| anyhow!("no network"))?;
            net.store_messages = on;
            net.name.clone()
        };
        self.networks.save()?;
        Ok(Reply::Ok(format!(
            "message storage for '{name}' is now {} ({}/messages/{}.jsonl)",
            if on { "ON" } else { "OFF" },
            self.home.display(),
            net_id.to_hex()
        )))
    }

    async fn cmd_kick(&mut self, user: String) -> Result<Reply> {
        let target = self.resolve_node(&user)?;
        let net_id = self.current;
        let (name, epoch, threshold) = {
            let net = self.networks.get(&net_id).ok_or_else(|| anyhow!("no network"))?;
            if net.is_default() {
                bail!("nobody can be kicked from [default]");
            }
            if !net.members.contains(&target) {
                bail!("{} is not a member of '{}'", self.display_of(&target), net.name);
            }
            (net.name.clone(), net.epoch, net.kick_threshold())
        };
        self.send_payload(net_id, None, NetPayload::KickVote { target, epoch })
            .await?;
        // Count our own ballot locally too.
        self.on_kick_vote(net_id, self.identity.id, target, epoch)
            .await?;
        Ok(Reply::Ok(format!(
            "[{name}] vote to kick {} cast (needs {threshold})",
            self.display_of(&target)
        )))
    }

    fn cmd_rename(&mut self, user: String, name: String) -> Result<Reply> {
        let id = self.resolve_node(&user)?;
        let contact = self
            .contacts
            .get_mut(&id)
            .ok_or_else(|| anyhow!("unknown user {id}"))?;
        contact.alias = Some(name.clone());
        store::save_contacts(&self.home, &self.contacts)?;
        Ok(Reply::Ok(format!("{id} will show as '{name}' on this device")))
    }

    fn cmd_switch(&mut self, name: String) -> Result<Reply> {
        let id = self.networks.resolve(&name)?;
        self.current = id;
        let label = self.current_network_name();
        let _ = self.events.try_send(Event::Context(label.clone()));
        Ok(Reply::Ok(format!("switched to [{label}]")))
    }

    async fn cmd_share_location(&mut self) -> Result<Reply> {
        let gps = self
            .location
            .ok_or_else(|| anyhow!("no location set - use --set-location [lat] [lon]"))?;
        let net = self.current;
        let name = self.current_network_name();
        self.send_payload(net, None, NetPayload::Gps(gps)).await?;
        Ok(Reply::Ok(format!(
            "[{name}] shared {:.5}, {:.5}",
            gps.lat, gps.lon
        )))
    }

    /// `--sos start` / `--sos stop` (plan.md §4 step 1.4).
    ///
    /// This is an **in-network** signal and nothing more. It raises an alert on the mesh
    /// around you. It does not, and must never, reach the operating system's emergency
    /// call path - plan.md §3.2 isolates the two precisely so a mesh test cannot dial
    /// real emergency services. Do not wire this to `tel:` anything.
    async fn cmd_sos(&mut self, active: bool) -> Result<Reply> {
        if self.sos == active {
            return Ok(Reply::Ok(format!(
                "SOS is already {}",
                if active { "active" } else { "off" }
            )));
        }
        self.sos = active;
        let net = self.current;
        let name = self.current_network_name();
        // Go out immediately at a longer TTL rather than waiting for the next beacon.
        self.send_payload_ttl(
            net,
            None,
            NetPayload::Sos {
                active,
                gps: self.location,
            },
            SOS_TTL,
        )
        .await?;
        self.send_hello().await?;
        Ok(Reply::Ok(if active {
            format!(
                "[{name}] SOS broadcast to the mesh (ttl {SOS_TTL}). This alerts nearby \
                 nodes only - it does not call emergency services. --sos stop to clear."
            )
        } else {
            format!("[{name}] SOS cleared")
        }))
    }

    /// `--status [code|name]` - one byte on the wire, never a string (plan.md §3.2).
    async fn cmd_status(&mut self, code: u8) -> Result<Reply> {
        if code != crate::status::NONE && crate::status::lookup(code).is_none() {
            bail!("unknown status code {code} - try --status with no argument for the list");
        }
        self.status = if code == crate::status::NONE {
            None
        } else {
            Some(code)
        };
        let net = self.current;
        let name = self.current_network_name();
        self.send_payload(net, None, NetPayload::Status { code }).await?;
        // Also fold it into the beacon, so a node that arrives later still learns it.
        self.send_hello().await?;
        Ok(Reply::Ok(format!(
            "[{name}] status: {} (1 byte, code {code})",
            crate::status::describe(code)
        )))
    }

    /// `--report-zone [lat] [lon] [level]` (plan.md §4 step 1.5).
    async fn cmd_report_zone(
        &mut self,
        lat: f64,
        lon: f64,
        verdict: zones::Verdict,
        radius_m: u32,
    ) -> Result<Reply> {
        if !(zones::MIN_RADIUS_M..=zones::MAX_RADIUS_M).contains(&radius_m) {
            bail!(
                "radius must be {} m to {} km",
                zones::MIN_RADIUS_M,
                zones::MAX_RADIUS_M / 1000
            );
        }
        let cell = zones::cell_for(lat, lon, self.zone_resolution)?;
        let now = now_ms();
        // `record_own` also moves this cell to the newest end of the 16-entry re-gossip
        // ring; a plain `record` would leave us republishing something we have replaced.
        self.zones
            .record_own(cell, self.identity.id, verdict, radius_m, now);
        self.zones.save()?;
        let net = self.current;
        let name = self.current_network_name();
        self.send_payload(
            net,
            None,
            NetPayload::Zone {
                cell,
                verdict: verdict.to_wire(),
                radius_m,
            },
        )
        .await?;
        let zone = self.zones.get(cell);
        let safe = zone.map(|z| z.safe_votes()).unwrap_or(0);
        let unsafe_votes = zone.map(|z| z.unsafe_votes()).unwrap_or(0);
        let aggregate = zone.map(|z| z.verdict()).unwrap_or(verdict);
        Ok(Reply::Ok(format!(
            "[{name}] reported {} within {} of cell {cell:x} - now reads {} ({safe} safe / {unsafe_votes} unsafe)",
            verdict.as_str(),
            zones::fmt_radius(radius_m),
            aggregate.as_str(),
        )))
    }

    fn cmd_link_filter(&mut self, users: Vec<String>) -> Result<Reply> {
        if users.is_empty() {
            self.router.set_link_filter(HashSet::new());
            return Ok(Reply::Ok("radio range filter cleared".to_string()));
        }
        let mut allowed = HashSet::new();
        for u in &users {
            allowed.insert(
                self.resolve_node(u)
                    .or_else(|_| NodeId::from_hex(u))
                    .map_err(|_| anyhow!("unknown user '{u}'"))?,
            );
        }
        let list: Vec<String> = allowed.iter().map(|i| i.to_hex()).collect();
        self.router.set_link_filter(allowed);
        Ok(Reply::Ok(format!(
            "radio range limited to [{}] - all other nodes are now unreachable except through relays",
            list.join(", ")
        )))
    }

    fn peer_views(&self) -> Vec<PeerView> {
        let current_members = self
            .networks
            .get(&self.current)
            .map(|n| n.members.clone())
            .unwrap_or_default();
        let is_default = self
            .networks
            .get(&self.current)
            .map(|n| n.is_default())
            .unwrap_or(true);
        let mut views: Vec<PeerView> = self
            .contacts
            .values()
            .map(|c| {
                let neighbor = self.router.neighbor(&c.id);
                let route = self.router.route(&c.id);
                // Unreachable, but we still hold where they were and when. plan.md §3.2:
                // a node whose battery died stays on the map as a ghost.
                let ghost = neighbor.is_none() && route.is_none();
                PeerView {
                    id: c.id,
                    display: c.display(),
                    direct: neighbor.is_some(),
                    hops: route.map(|r| r.hops),
                    rtt_ms: neighbor.and_then(|n| n.rtt_ms),
                    rssi: neighbor.and_then(|n| n.rssi),
                    distance_m: match (self.location, c.gps) {
                        (Some(mine), Some(theirs)) => Some(haversine_m(&mine, &theirs)),
                        _ => None,
                    },
                    gps: c.gps,
                    last_seen_ms: c.last_seen_ms,
                    in_current_network: is_default || current_members.contains(&c.id),
                    battery: c.battery,
                    status: c.status,
                    sos: c.sos,
                    ghost,
                }
            })
            .collect();
        // Nearest first: GPS distance when we have it, else hop count, else latency.
        // Ghosts sort below every reachable peer regardless of how close they were.
        views.sort_by(|a, b| {
            let key = |p: &PeerView| {
                (
                    p.ghost as u8 as f64,
                    p.distance_m.unwrap_or(f64::MAX),
                    p.hops.unwrap_or(u8::MAX) as f64,
                    p.rtt_ms.unwrap_or(u64::MAX) as f64,
                )
            };
            let (ka, kb) = (key(a), key(b));
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        views
    }

    fn network_views(&self) -> Vec<NetworkView> {
        self.networks
            .iter()
            .map(|n| NetworkView {
                id: n.id,
                name: n.name.clone(),
                members: n.members.iter().map(|m| self.display_of(m)).collect(),
                member_count: n.members.len(),
                epoch: n.epoch,
                store_messages: n.store_messages,
                active: n.id == self.current,
                is_default: n.is_default(),
            })
            .collect()
    }

    fn route_views(&self) -> Vec<RouteView> {
        let now = now_ms();
        let mut views: Vec<RouteView> = self
            .router
            .routes()
            .map(|r| RouteView {
                dest: r.dest,
                display: self.display_of(&r.dest),
                next_hop: r.next_hop,
                next_hop_display: self.display_of(&r.next_hop),
                hops: r.hops,
                age_ms: now.saturating_sub(r.last_seen_ms),
            })
            .collect();
        views.sort_by_key(|r| (r.hops, r.age_ms));
        views
    }
}
