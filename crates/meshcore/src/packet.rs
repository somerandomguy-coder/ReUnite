//! Wire format.
//!
//! Two layers:
//!   * `Frame`  - what actually goes into one datagram / BLE write. It names the *link*
//!                sender (our direct neighbour) and carries exactly one `Packet`.
//!   * `Packet` - the end-to-end unit that is flooded or routed across the mesh. It is
//!                signed by its origin and carries TTL + path for loop-free forwarding
//!                (plan.md §6.2 "Mesh Network Flooding").

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::crypto::SealedBox;
use crate::types::{Gps, MsgId, NetworkId, NodeId};

pub const MAGIC: u32 = 0x4d45_5348; // "MESH"
pub const VERSION: u8 = 3;
pub const DEFAULT_TTL: u8 = 8;
/// Keep frames comfortably inside a single UDP datagram (and, later, a BLE MTU chain).
pub const MAX_FRAME_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub magic: u32,
    pub version: u8,
    /// The neighbour that physically handed us this frame (not the author).
    pub link_from: NodeId,
    /// Random per *process*, so a node can tell its own multicast echo apart from a
    /// second process that loaded the same identity (i.e. the same `--home` twice).
    pub instance: u64,
    pub packet: Packet,
}

impl Frame {
    pub fn new(link_from: NodeId, instance: u64, packet: Packet) -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            link_from,
            instance,
            packet,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let bytes = bincode::serialize(self)?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(anyhow!("frame too large: {} bytes", bytes.len()));
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        // Check the fixed 5 byte header before deserializing the rest: a node running an
        // older build must be reported as a version mismatch, not as parser garbage.
        if bytes.len() < 5 {
            return Err(anyhow!("frame too short"));
        }
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != MAGIC {
            return Err(anyhow!("not a mesh frame"));
        }
        let version = bytes[4];
        if version != VERSION {
            return Err(anyhow!(
                "unsupported protocol version {version}, this build speaks {VERSION}"
            ));
        }
        Ok(bincode::deserialize(bytes)?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Packet {
    pub id: MsgId,
    pub origin: NodeId,
    /// `None` = flood to everyone, `Some(id)` = deliver to that node.
    pub dest: Option<NodeId>,
    pub sent_at_ms: u64,
    pub body: Body,
    /// Ed25519 signature by `origin` over the immutable fields above.
    pub sig: Vec<u8>,
    // ---- mutable in transit, deliberately outside the signature ----
    pub ttl: u8,
    /// Relays that already forwarded this packet, in order. Used to learn reverse routes.
    pub path: Vec<NodeId>,
}

/// The signed view of a packet: everything a relay must not be able to tamper with.
#[derive(Serialize)]
struct SignedView<'a> {
    id: &'a MsgId,
    origin: &'a NodeId,
    dest: &'a Option<NodeId>,
    sent_at_ms: u64,
    body: &'a Body,
}

impl Packet {
    pub fn signing_bytes(&self) -> Vec<u8> {
        bincode::serialize(&SignedView {
            id: &self.id,
            origin: &self.origin,
            dest: &self.dest,
            sent_at_ms: self.sent_at_ms,
            body: &self.body,
        })
        .expect("packet fields are serializable")
    }

    /// Hops travelled so far (1 == came straight from the origin).
    pub fn hops(&self) -> u8 {
        (self.path.len() as u8).saturating_add(1)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Body {
    /// Periodic presence beacon on the default network: keys, name hint, public GPS.
    Hello(Hello),
    /// Latency probe between direct neighbours.
    Ping { nonce: u64 },
    Pong { nonce: u64, echo_sent_ms: u64 },
    /// Any traffic that belongs to a network, encrypted with that network's key.
    Envelope(Envelope),
    /// A network key sealed to one recipient's X25519 key (invite or re-key).
    Invite(Invite),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hello {
    pub ed_pub: [u8; 32],
    pub x_pub: [u8; 32],
    /// What the node calls itself. Local `--rename` aliases always win over this.
    pub name: Option<String>,
    pub gps: Option<Gps>,
    /// Charge 0..=100, `None` when the platform will not report it (plan.md §3.1).
    pub battery: Option<u8>,
    /// In-network SOS flag. Mirrors `beacon::FLAG_SOS`.
    pub sos: bool,
    /// Last pre-canned status code, so a node that arrives late still learns it.
    pub status: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub network: NetworkId,
    /// Key generation; bumped on every kick-driven re-key.
    pub epoch: u32,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invite {
    pub network: NetworkId,
    pub epoch: u32,
    pub sealed: SealedBox,
}

/// Plaintext that lives inside an `Envelope`, i.e. only readable by network members.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetPayload {
    Chat { text: String },
    Direct { text: String },
    Gps(Gps),
    /// Authoritative membership snapshot, sent after invites and re-keys.
    Members { members: Vec<NodeId>, epoch: u32 },
    /// One signed ballot to remove `target` (plan.md §4 step 1.4).
    KickVote { target: NodeId, epoch: u32 },
    /// End-to-end delivery receipt for a direct message.
    Ack { msg: MsgId },
    /// A pre-canned panic message. One byte, never a string (plan.md §3.2).
    Status { code: u8 },
    /// In-network SOS. Explicitly *not* the OS emergency-services SOS.
    Sos { active: bool, gps: Option<Gps> },
    /// One node's safety report for one H3 cell (plan.md §4 step 1.5).
    Zone { cell: u64, level: u8 },
}

/// Plaintext inside an `Invite` sealed box.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitePayload {
    pub network: NetworkId,
    pub name: String,
    pub creator: NodeId,
    pub epoch: u32,
    pub key: [u8; 32],
    pub members: Vec<NodeId>,
}
