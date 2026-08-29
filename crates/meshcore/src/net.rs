//! Network (channel) membership and keys - plan.md §4 steps 1.3 and 1.4.
//!
//! `[default]` is the lobby everyone lands in: it uses a well-known key derived from a
//! constant, so it is effectively public - it exists so that discovery, GPS beacons and
//! public shouts all travel through the same encrypted envelope path as private traffic.
//!
//! A private network is a name + a random 32 byte key + a member list. The key only ever
//! leaves a device inside a sealed box addressed to one invited member, so a node that was
//! never invited cannot decrypt anything even while it relays the packets.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::crypto::{self, SymKey};
use crate::packet::InvitePayload;
use crate::store::{self, NetworkRecord, NetworksFile};
use crate::types::{NetworkId, NodeId};

pub const DEFAULT_NETWORK_NAME: &str = "default";

pub fn default_network_id() -> NetworkId {
    NetworkId::zero()
}

/// Well-known key for `[default]`. Not a secret: it only frames public traffic.
pub fn default_network_key() -> SymKey {
    crypto::derive_key(b"meshnet-default-network", "meshnet/default-network/v1")
}

#[derive(Clone, Debug)]
pub struct Network {
    pub id: NetworkId,
    pub name: String,
    pub creator: NodeId,
    pub epoch: u32,
    pub key: SymKey,
    /// Older generations, kept so in-flight packets sent before a re-key still open.
    pub previous_keys: HashMap<u32, SymKey>,
    pub members: BTreeSet<NodeId>,
    pub store_messages: bool,
    /// Live kick tally: (target, epoch) -> voters. Runtime only, never persisted.
    pub votes: HashMap<(NodeId, u32), BTreeSet<NodeId>>,
}

impl Network {
    pub fn is_default(&self) -> bool {
        self.id == default_network_id()
    }

    pub fn key_for_epoch(&self, epoch: u32) -> Option<SymKey> {
        if epoch == self.epoch {
            Some(self.key)
        } else {
            self.previous_keys.get(&epoch).copied()
        }
    }

    /// Votes needed to remove someone: `>= size / 2` (plan.md §4 step 1.4).
    pub fn kick_threshold(&self) -> usize {
        let size = self.members.len().max(1);
        size.div_ceil(2)
    }

    /// After a kick the smallest remaining member id mints and distributes the new key.
    /// Deterministic, so every member agrees on who does it without extra messaging.
    pub fn rekey_leader(&self, removing: &NodeId) -> Option<NodeId> {
        self.members.iter().find(|m| *m != removing).copied()
    }

    fn to_record(&self) -> NetworkRecord {
        NetworkRecord {
            id: self.id.to_hex(),
            name: self.name.clone(),
            creator: self.creator.to_hex(),
            epoch: self.epoch,
            key: hex::encode(self.key),
            previous_keys: self
                .previous_keys
                .iter()
                .map(|(e, k)| (e.to_string(), hex::encode(k)))
                .collect(),
            members: self.members.iter().map(|m| m.to_hex()).collect(),
            store_messages: self.store_messages,
        }
    }

    fn from_record(rec: &NetworkRecord) -> Result<Self> {
        let key: SymKey = hex::decode(&rec.key)?
            .try_into()
            .map_err(|_| anyhow!("network {}: key must be 32 bytes", rec.name))?;
        let mut previous_keys = HashMap::new();
        for (epoch, hexed) in &rec.previous_keys {
            let k: SymKey = hex::decode(hexed)?
                .try_into()
                .map_err(|_| anyhow!("network {}: previous key must be 32 bytes", rec.name))?;
            previous_keys.insert(epoch.parse::<u32>()?, k);
        }
        Ok(Self {
            id: NetworkId::from_hex(&rec.id)?,
            name: rec.name.clone(),
            creator: NodeId::from_hex(&rec.creator)?,
            epoch: rec.epoch,
            key,
            previous_keys,
            members: rec
                .members
                .iter()
                .map(|m| NodeId::from_hex(m))
                .collect::<Result<_>>()?,
            store_messages: rec.store_messages,
            votes: HashMap::new(),
        })
    }
}

pub struct NetworkBook {
    home: PathBuf,
    nets: BTreeMap<NetworkId, Network>,
}

impl NetworkBook {
    pub fn load(home: &Path, me: NodeId) -> Result<Self> {
        let file = store::load_networks(home)?;
        let mut nets = BTreeMap::new();
        for rec in &file.networks {
            let net = Network::from_record(rec)?;
            nets.insert(net.id, net);
        }
        nets.entry(default_network_id()).or_insert_with(|| Network {
            id: default_network_id(),
            name: DEFAULT_NETWORK_NAME.to_string(),
            creator: me,
            epoch: 0,
            key: default_network_key(),
            previous_keys: HashMap::new(),
            members: BTreeSet::new(),
            store_messages: false,
            votes: HashMap::new(),
        });
        Ok(Self {
            home: home.to_path_buf(),
            nets,
        })
    }

    pub fn save(&self) -> Result<()> {
        let networks = self
            .nets
            .values()
            .filter(|n| !n.is_default())
            .map(Network::to_record)
            .collect();
        store::save_networks(&self.home, &NetworksFile { networks })
    }

    pub fn get(&self, id: &NetworkId) -> Option<&Network> {
        self.nets.get(id)
    }

    pub fn get_mut(&mut self, id: &NetworkId) -> Option<&mut Network> {
        self.nets.get_mut(id)
    }

    pub fn by_name(&self, name: &str) -> Option<&Network> {
        self.nets.values().find(|n| n.name.eq_ignore_ascii_case(name))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Network> {
        self.nets.values()
    }

    /// Resolve a user-typed network reference: exact name, or a network id in hex.
    pub fn resolve(&self, needle: &str) -> Result<NetworkId> {
        if let Some(net) = self.by_name(needle) {
            return Ok(net.id);
        }
        if let Ok(id) = NetworkId::from_hex(needle) {
            if self.nets.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(anyhow!("unknown network '{needle}' (try --networks)"))
    }

    /// `--create-network [name]` (plan.md §5).
    pub fn create(&mut self, name: &str, creator: NodeId) -> Result<NetworkId> {
        if name.eq_ignore_ascii_case(DEFAULT_NETWORK_NAME) {
            return Err(anyhow!("'{DEFAULT_NETWORK_NAME}' is reserved"));
        }
        if self.by_name(name).is_some() {
            return Err(anyhow!("network '{name}' already exists here"));
        }
        let id = NetworkId::derive(name, &creator, &crypto::random_bytes::<16>());
        let mut members = BTreeSet::new();
        members.insert(creator);
        self.nets.insert(
            id,
            Network {
                id,
                name: name.to_string(),
                creator,
                epoch: 0,
                key: crypto::random_key(),
                previous_keys: HashMap::new(),
                members,
                store_messages: false,
                votes: HashMap::new(),
            },
        );
        self.save()?;
        Ok(id)
    }

    /// Apply an invite (or a re-key for a network we already know).
    /// Returns true when this is the first time we join.
    pub fn accept_invite(&mut self, payload: &InvitePayload) -> Result<bool> {
        match self.nets.get_mut(&payload.network) {
            Some(existing) => {
                if payload.epoch < existing.epoch {
                    return Ok(false); // stale re-key, ignore
                }
                if payload.epoch > existing.epoch {
                    existing.previous_keys.insert(existing.epoch, existing.key);
                    existing.epoch = payload.epoch;
                    existing.key = payload.key;
                    existing.votes.clear();
                }
                existing.members = payload.members.iter().copied().collect();
                self.save()?;
                Ok(false)
            }
            None => {
                self.nets.insert(
                    payload.network,
                    Network {
                        id: payload.network,
                        name: payload.name.clone(),
                        creator: payload.creator,
                        epoch: payload.epoch,
                        key: payload.key,
                        previous_keys: HashMap::new(),
                        members: payload.members.iter().copied().collect(),
                        store_messages: false,
                        votes: HashMap::new(),
                    },
                );
                self.save()?;
                Ok(true)
            }
        }
    }

    /// Rotate to a fresh key after a successful kick. Returns the new epoch.
    pub fn rekey(&mut self, id: &NetworkId, removed: &NodeId) -> Result<u32> {
        let net = self
            .nets
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown network"))?;
        net.previous_keys.insert(net.epoch, net.key);
        net.epoch += 1;
        net.key = crypto::random_key();
        net.members.remove(removed);
        net.votes.clear();
        let epoch = net.epoch;
        self.save()?;
        Ok(epoch)
    }
}
