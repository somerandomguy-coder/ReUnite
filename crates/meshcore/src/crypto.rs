//! Cryptographic primitives (plan.md §3.2).
//!
//! * Ed25519  - node signing identity (authenticates packets, votes, invites).
//! * X25519   - key agreement used to seal a network's symmetric key to one recipient.
//! * ChaCha20-Poly1305 - symmetric AEAD for all network traffic.
//! * HKDF-SHA256 - key derivation.

use anyhow::{anyhow, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as XPublicKey, StaticSecret};

pub type SymKey = [u8; 32];

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

pub fn random_key() -> SymKey {
    random_bytes::<32>()
}

/// Derive a 32 byte key from input keying material and a context string.
pub fn derive_key(ikm: &[u8], info: &str) -> SymKey {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info.as_bytes(), &mut okm)
        .expect("32 bytes is a valid HKDF length");
    okm
}

// ---------------------------------------------------------------- signatures

pub fn signing_key_from_bytes(bytes: &[u8; 32]) -> SigningKey {
    SigningKey::from_bytes(bytes)
}

pub fn new_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

pub fn sign(key: &SigningKey, message: &[u8]) -> Vec<u8> {
    key.sign(message).to_bytes().to_vec()
}

pub fn verify(public: &[u8; 32], message: &[u8], signature: &[u8]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(public).map_err(|e| anyhow!("bad public key: {e}"))?;
    let sig = Signature::from_slice(signature).map_err(|e| anyhow!("bad signature: {e}"))?;
    vk.verify(message, &sig)
        .map_err(|_| anyhow!("signature verification failed"))
}

// -------------------------------------------------------------- key exchange

pub fn new_exchange_secret() -> StaticSecret {
    StaticSecret::random_from_rng(OsRng)
}

pub fn exchange_public(secret: &StaticSecret) -> [u8; 32] {
    XPublicKey::from(secret).to_bytes()
}

/// A one-shot, anonymous-sender sealed box: ephemeral X25519 -> HKDF -> ChaCha20-Poly1305.
/// Used to hand a network key to exactly one invited member (plan.md §4 step 1.3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SealedBox {
    pub ephemeral_pub: [u8; 32],
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn seal_to(recipient_pub: &[u8; 32], plaintext: &[u8]) -> Result<SealedBox> {
    let ephemeral = new_exchange_secret();
    let ephemeral_pub = exchange_public(&ephemeral);
    let shared = ephemeral.diffie_hellman(&XPublicKey::from(*recipient_pub));
    let mut ikm = Vec::with_capacity(64);
    ikm.extend_from_slice(shared.as_bytes());
    ikm.extend_from_slice(&ephemeral_pub);
    ikm.extend_from_slice(recipient_pub);
    let key = derive_key(&ikm, "meshnet/sealed-box/v1");
    let nonce = random_bytes::<12>();
    let ciphertext = sym_encrypt_with_nonce(&key, &nonce, plaintext)?;
    Ok(SealedBox {
        ephemeral_pub,
        nonce,
        ciphertext,
    })
}

pub fn open_sealed(secret: &StaticSecret, sealed: &SealedBox) -> Result<Vec<u8>> {
    let our_pub = exchange_public(secret);
    let shared = secret.diffie_hellman(&XPublicKey::from(sealed.ephemeral_pub));
    let mut ikm = Vec::with_capacity(64);
    ikm.extend_from_slice(shared.as_bytes());
    ikm.extend_from_slice(&sealed.ephemeral_pub);
    ikm.extend_from_slice(&our_pub);
    let key = derive_key(&ikm, "meshnet/sealed-box/v1");
    sym_decrypt(&key, &sealed.nonce, &sealed.ciphertext)
}

// ------------------------------------------------------------------- symmetric

pub fn sym_encrypt(key: &SymKey, plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>)> {
    let nonce = random_bytes::<12>();
    let ct = sym_encrypt_with_nonce(key, &nonce, plaintext)?;
    Ok((nonce, ct))
}

fn sym_encrypt_with_nonce(key: &SymKey, nonce: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(Nonce::from_slice(nonce), plaintext)
        .map_err(|_| anyhow!("encryption failed"))
}

pub fn sym_decrypt(key: &SymKey, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow!("decryption failed (wrong key or tampered packet)"))
}
