//! Node key
#![cfg(feature = "node-key")]
use crate::result::{Error, Result};
use libp2p::{
    identity::{ed25519, PublicKey},
    PeerId,
};

/// Generate node key
pub fn generate() -> (ed25519::Keypair, PeerId) {
    let pair = ed25519::Keypair::generate();
    let public = pair.public();

    (pair, PublicKey::Ed25519(public).to_peer_id())
}

/// Get inspect of node key from secret
pub fn inspect(mut data: Vec<u8>) -> Result<(ed25519::Keypair, PeerId)> {
    let secret = ed25519::SecretKey::from_bytes(&mut data).map_err(|_| Error::BadNodeKey)?;
    let pair = ed25519::Keypair::from(secret);
    let public = pair.public();

    Ok((pair, PublicKey::Ed25519(public).to_peer_id()))
}
