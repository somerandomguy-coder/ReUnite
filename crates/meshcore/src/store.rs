//! On-disk state (plan.md §2 "Data Storage").
//!
//! Everything lives under one home directory (default `~/.meshnet`, override with
//! `--home`), so a laptop can run several independent nodes for testing:
//!
//! ```text
//! identity.json          this node's UUID + keys
//! contacts.json          node id -> public keys, local alias, last GPS
//! networks.json          private networks we belong to, including their symmetric keys
//! zones.json             aggregated H3 safe-zone reports and their consensus counts
//! messages/<net>.jsonl   append-only log, only when --enable-storing is on
//! ```

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::types::{Gps, NetworkId, NodeId};

/// Resolve the node home directory, creating it if needed.
pub fn resolve_home(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let home = match explicit {
        Some(p) => p,
        None => match std::env::var_os("MESHNET_HOME") {
            Some(p) => PathBuf::from(p),
            None => {
                let base = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .context("cannot determine home directory; pass --home")?;
                PathBuf::from(base).join(".meshnet")
            }
        },
    };
    fs::create_dir_all(&home).with_context(|| format!("creating {}", home.display()))?;
    Ok(home)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Write atomically: a half-written networks.json would lose a network key.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

// ------------------------------------------------------------------- contacts

#[derive(Clone, Debug)]
pub struct Contact {
    pub id: NodeId,
    pub ed_pub: [u8; 32],
    pub x_pub: [u8; 32],
    /// Local alias set with `--rename`; only ever shown on this device.
    pub alias: Option<String>,
    /// The name the peer advertises for itself.
    pub self_name: Option<String>,
    pub last_seen_ms: u64,
    pub gps: Option<Gps>,
    /// Charge 0..=100 as last advertised.
    pub battery: Option<u8>,
    /// Last pre-canned status code (`status.rs`), `None` when never set or cleared.
    pub status: Option<u8>,
    /// Whether their last beacon carried the in-network SOS flag.
    pub sos: bool,
}

impl Contact {
    pub fn display(&self) -> String {
        match (&self.alias, &self.self_name) {
            (Some(a), _) => a.clone(),
            (None, Some(n)) => format!("~{n}"),
            (None, None) => self.id.to_hex(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ContactRecord {
    id: String,
    ed_pub: String,
    x_pub: String,
    #[serde(default)]
    alias: Option<String>,
    #[serde(default)]
    self_name: Option<String>,
    #[serde(default)]
    last_seen_ms: u64,
    #[serde(default)]
    gps: Option<Gps>,
    #[serde(default)]
    battery: Option<u8>,
    #[serde(default)]
    status: Option<u8>,
    #[serde(default)]
    sos: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct ContactsFile {
    contacts: Vec<ContactRecord>,
}

fn to_key32(hexed: &str) -> Result<[u8; 32]> {
    let raw = hex::decode(hexed)?;
    raw.try_into()
        .map_err(|_| anyhow::anyhow!("expected a 32 byte key"))
}

pub fn load_contacts(home: &Path) -> Result<HashMap<NodeId, Contact>> {
    let file: ContactsFile = read_json(&home.join("contacts.json"))?.unwrap_or_default();
    let mut out = HashMap::new();
    for rec in file.contacts {
        let id = NodeId::from_hex(&rec.id)?;
        out.insert(
            id,
            Contact {
                id,
                ed_pub: to_key32(&rec.ed_pub)?,
                x_pub: to_key32(&rec.x_pub)?,
                alias: rec.alias,
                self_name: rec.self_name,
                last_seen_ms: rec.last_seen_ms,
                gps: rec.gps,
                battery: rec.battery,
                status: rec.status,
                sos: rec.sos,
            },
        );
    }
    Ok(out)
}

pub fn save_contacts(home: &Path, contacts: &HashMap<NodeId, Contact>) -> Result<()> {
    let mut records: Vec<ContactRecord> = contacts
        .values()
        .map(|c| ContactRecord {
            id: c.id.to_hex(),
            ed_pub: hex::encode(c.ed_pub),
            x_pub: hex::encode(c.x_pub),
            alias: c.alias.clone(),
            self_name: c.self_name.clone(),
            last_seen_ms: c.last_seen_ms,
            gps: c.gps,
            battery: c.battery,
            status: c.status,
            sos: c.sos,
        })
        .collect();
    records.sort_by(|a, b| a.id.cmp(&b.id));
    write_json(&home.join("contacts.json"), &ContactsFile { contacts: records })
}

// ------------------------------------------------------------------- networks

#[derive(Serialize, Deserialize)]
pub struct NetworkRecord {
    pub id: String,
    pub name: String,
    pub creator: String,
    pub epoch: u32,
    pub key: String,
    #[serde(default)]
    pub previous_keys: HashMap<String, String>,
    pub members: Vec<String>,
    #[serde(default)]
    pub store_messages: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct NetworksFile {
    pub networks: Vec<NetworkRecord>,
}

pub fn load_networks(home: &Path) -> Result<NetworksFile> {
    Ok(read_json(&home.join("networks.json"))?.unwrap_or_default())
}

pub fn save_networks(home: &Path, file: &NetworksFile) -> Result<()> {
    write_json(&home.join("networks.json"), file)
}

// ------------------------------------------------------------- message history

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredMessage {
    pub ts_ms: u64,
    pub network: String,
    pub network_name: String,
    pub kind: String,
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
    pub text: String,
}

fn message_log_path(home: &Path, network: &NetworkId) -> PathBuf {
    home.join("messages").join(format!("{}.jsonl", network.to_hex()))
}

/// Append one message to a network's log. Only called when storing is enabled.
pub fn append_message(home: &Path, network: &NetworkId, msg: &StoredMessage) -> Result<()> {
    let path = message_log_path(home, network);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(msg)?)?;
    Ok(())
}

pub fn read_messages(home: &Path, network: &NetworkId, limit: usize) -> Result<Vec<StoredMessage>> {
    let path = message_log_path(home, network);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut all: Vec<StoredMessage> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if all.len() > limit {
        all = all.split_off(all.len() - limit);
    }
    Ok(all)
}
