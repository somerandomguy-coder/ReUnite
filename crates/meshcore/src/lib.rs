//! Core of the offline P2P emergency mesh (plan.md Phase 1).
//!
//! Layers, bottom to top:
//!   `transport` - moves opaque frames between machines (UDP today, BLE/Wi-Fi Direct later)
//!   `packet`    - frame + packet wire format, TTL and path recording
//!   `router`    - neighbours, learned routes, duplicate suppression
//!   `crypto`/`net` - identities, network keys, invites, kick votes
//!   `node`      - the actor that ties it together and exposes Command/Event
//!
//! The CLI in `meshcli` is a thin shell over `node`; a mobile UI would bind to the
//! same `NodeHandle` API.

pub mod crypto;
pub mod geo;
pub mod identity;
pub mod net;
pub mod node;
pub mod packet;
pub mod router;
pub mod store;
pub mod transport;
pub mod types;

pub use node::{Command, Event, Node, NodeConfig, NodeHandle, Reply};
pub use types::{Gps, NetworkId, NodeId};
