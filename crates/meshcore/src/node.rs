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

const OUTBOX_RETRY_MS: u64 = 15_000;
const OUTBOX_EXPIRY_MS: u64 = 120_000;

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
}

#[derive(Clone, Debug)]
pub enum Reply {
    Ok(String),
    Peers(Vec<PeerView>),
    Networks(Vec<NetworkView>),
    Routes(Vec<RouteView>),
    History(Vec<StoredMessage>),
    Whoami(WhoamiView),
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
                _ = hello.tick() => { let _ = self.send_hello().await; }
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
        });
        let packet = self.make_packet(None, body, DEFAULT_TTL);
        self.dispatch(packet).await
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
        let body = self.seal(&network, &payload)?;
        let packet = self.make_packet(dest, body, DEFAULT_TTL);
        let id = packet.id;
        self.dispatch(packet).await?;
        Ok(id)
    }

    // -------------------------------------------------------------- inbound

    async fn on_frame(&mut self, bytes: &[u8], from: SocketAddr) -> Result<()> {
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
        let display = entry.display();
        self.contacts_dirty = true;

        // Everyone we can hear is a member of [default]; private networks are explicit.
        if let Some(def) = self.networks.get_mut(&default_network_id()) {
            def.members.insert(id);
        }

        if is_new {
            let _ = self.events.try_send(Event::PeerJoined { id, display: display.clone() });
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
                }
            })
            .collect();
        // Nearest first: GPS distance when we have it, else hop count, else latency.
        views.sort_by(|a, b| {
            let key = |p: &PeerView| {
                (
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
