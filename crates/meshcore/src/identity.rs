//! Persistent node identity (plan.md §4 step 1.1).
//!
//! On first launch we generate a random UUID, hash it into a short Node ID, and keep
//! an Ed25519 signing key plus an X25519 key-agreement key next to it on disk.

use std::path::Path;

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use x25519_dalek::StaticSecret;

use crate::crypto;
use crate::store::{read_json, write_json};
use crate::types::NodeId;

#[derive(Serialize, Deserialize)]
struct IdentityFile {
    uuid: String,
    node_id: String,
    ed25519_secret: String,
    x25519_secret: String,
}

pub struct Identity {
    pub uuid: String,
    pub id: NodeId,
    pub signing: SigningKey,
    pub exchange: StaticSecret,
}

impl Identity {
    /// Load the identity from `<home>/identity.json`, creating it on first run.
    pub fn load_or_create(home: &Path) -> Result<Self> {
        let path = home.join("identity.json");
        if let Some(file) = read_json::<IdentityFile>(&path)? {
            let ed = hex::decode(&file.ed25519_secret).context("identity.json: ed25519_secret")?;
            let x = hex::decode(&file.x25519_secret).context("identity.json: x25519_secret")?;
            let ed: [u8; 32] = ed
                .try_into()
                .map_err(|_| anyhow::anyhow!("ed25519_secret must be 32 bytes"))?;
            let x: [u8; 32] = x
                .try_into()
                .map_err(|_| anyhow::anyhow!("x25519_secret must be 32 bytes"))?;
            return Ok(Self {
                id: NodeId::from_hex(&file.node_id)?,
                uuid: file.uuid,
                signing: crypto::signing_key_from_bytes(&ed),
                exchange: StaticSecret::from(x),
            });
        }

        let uuid = uuid::Uuid::new_v4().to_string();
        let id = NodeId::from_uuid(&uuid);
        let signing = crypto::new_signing_key();
        let exchange = crypto::new_exchange_secret();
        let file = IdentityFile {
            uuid: uuid.clone(),
            node_id: id.to_hex(),
            ed25519_secret: hex::encode(signing.to_bytes()),
            x25519_secret: hex::encode(exchange.to_bytes()),
        };
        write_json(&path, &file)?;
        Ok(Self {
            uuid,
            id,
            signing,
            exchange,
        })
    }

    pub fn ed_public(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    pub fn x_public(&self) -> [u8; 32] {
        crypto::exchange_public(&self.exchange)
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        crypto::sign(&self.signing, message)
    }
}
