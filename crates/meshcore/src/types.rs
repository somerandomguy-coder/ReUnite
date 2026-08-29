//! Shared primitive types: node / network / message identifiers and time helpers.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

macro_rules! hex_id {
    ($name:ident, $len:expr) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub [u8; $len]);

        impl $name {
            pub const LEN: usize = $len;

            pub fn zero() -> Self {
                Self([0u8; $len])
            }

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }

            pub fn from_hex(s: &str) -> Result<Self> {
                let raw = hex::decode(s.trim())?;
                if raw.len() != $len {
                    return Err(anyhow!(
                        "expected {} hex chars, got {}",
                        $len * 2,
                        s.trim().len()
                    ));
                }
                let mut out = [0u8; $len];
                out.copy_from_slice(&raw);
                Ok(Self(out))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.to_hex())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.to_hex())
            }
        }
    };
}

hex_id!(NodeId, 8);
hex_id!(NetworkId, 8);
hex_id!(MsgId, 16);

impl NodeId {
    /// Node IDs are the first 8 bytes of SHA-256 over a locally generated UUID.
    /// See plan.md §2 "MAC Address Randomization" for why we hash a UUID and not a MAC.
    pub fn from_uuid(uuid: &str) -> Self {
        let digest = Sha256::digest(uuid.as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&digest[..8]);
        Self(out)
    }
}

impl NetworkId {
    /// Deterministic-per-creation network id: SHA-256(name || creator || nonce).
    pub fn derive(name: &str, creator: &NodeId, nonce: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        hasher.update(creator.0);
        hasher.update(nonce);
        let digest = hasher.finalize();
        let mut out = [0u8; 8];
        out.copy_from_slice(&digest[..8]);
        Self(out)
    }
}

impl MsgId {
    pub fn random() -> Self {
        let mut out = [0u8; 16];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut out);
        Self(out)
    }
}

/// A GPS fix as shared over the mesh.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Gps {
    pub lat: f64,
    pub lon: f64,
    pub ts_ms: u64,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
