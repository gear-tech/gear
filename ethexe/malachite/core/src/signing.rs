// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! secp256k1 / ECDSA signing primitives plus the libp2p identity
//! derivation that the Malachite swarm uses.
//!
//! The node's master secret enters the service via
//! [`crate::MalachiteConfig::validator_secret`] (a
//! `gsigner::secp256k1::PrivateKey`). The 32 raw bytes drive two
//! separate identities:
//!
//! - the consensus signer ([`MalachiteSigner`]) — signs Malachite
//!   votes / proposals / `Fin` parts;
//! - a domain-separated libp2p keypair — independent peer-id so a
//!   process running another libp2p swarm under the same key doesn't
//!   collide.
//!
//! Address derivation is the standard
//! `keccak256(uncompressed_pubkey[1..])[12..]` flow. The 20-byte
//! address sits inside [`crate::Address`] as a gsigner newtype.
//!
//! The malachite-side `SigningProvider<MalachiteCtx>` impl for
//! [`MalachiteSigner`] lives in [`crate::context`] alongside the
//! `Context` type it parametrises.

use anyhow::{Context as _, Result};
use libp2p_identity::{Keypair, PeerId};
use sha3::{Digest, Keccak256};

use malachitebft_signing_ecdsa::K256Config;

/// Concrete ECDSA private key on the k256 curve.
pub type PrivateKey = malachitebft_signing_ecdsa::PrivateKey<K256Config>;

/// Concrete ECDSA public key on the k256 curve.
pub type PublicKey = malachitebft_signing_ecdsa::PublicKey<K256Config>;

/// Concrete ECDSA signature on the k256 curve.
pub type Signature = malachitebft_signing_ecdsa::Signature<K256Config>;

/// Local signing helper, the consensus side of the validator
/// identity. Owns the private key for the lifetime of the service
/// and exposes the small set of operations the malachite layer
/// needs.
#[derive(Debug)]
pub struct MalachiteSigner {
    private_key: PrivateKey,
}

impl MalachiteSigner {
    pub fn new(private_key: PrivateKey) -> Self {
        Self { private_key }
    }

    /// Construct from a raw 32-byte secret.
    pub fn from_bytes(secret: &[u8; 32]) -> Result<Self> {
        let pk = private_key_from_bytes(secret).context("constructing MalachiteSigner")?;
        Ok(Self::new(pk))
    }

    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    pub fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.private_key.sign(data)
    }

    pub fn verify(&self, data: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
        public_key.verify(data, signature).is_ok()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Pack a [`Signature`] into a `Vec<u8>` (raw `r || s` for the
/// k256 curve, 64 bytes). Helper used by the SCALE codec layer.
pub fn signature_to_vec(s: &Signature) -> Vec<u8> {
    s.to_vec()
}

/// Reverse of [`signature_to_vec`].
pub fn signature_from_vec(bytes: &[u8]) -> Result<Signature> {
    Signature::from_slice(bytes).map_err(|e| anyhow::anyhow!("decoding signature from bytes: {e}"))
}

/// Construct an ECDSA private key from a raw 32-byte secret. Returns
/// an error if the bytes are not a valid k256 scalar (zero or ≥ curve
/// order); for randomly drawn secrets this is overwhelmingly unlikely
/// (≈ 2^-128) but real input may come from anywhere.
pub fn private_key_from_bytes(secret: &[u8; 32]) -> Result<PrivateKey> {
    PrivateKey::from_slice(secret)
        .map_err(|e| anyhow::anyhow!("constructing ECDSA private key: {e}"))
}

/// Convert a `gsigner` secp256k1 [`PrivateKey`] into the malachite-
/// side [`PrivateKey`]. Both are k256-backed, so this is a
/// bytes-roundtrip.
pub fn private_key_from_gsigner(
    pk: &gsigner::schemes::secp256k1::PrivateKey,
) -> Result<PrivateKey> {
    private_key_from_bytes(&pk.to_bytes())
}

/// Convert a `gsigner` secp256k1 [`PublicKey`] into the malachite-
/// side [`PublicKey`]. Both are k256-backed; gsigner stores the
/// 33-byte SEC1 compressed form, which malachite accepts via
/// `from_sec1_bytes`.
pub fn public_key_from_gsigner(pk: &gsigner::schemes::secp256k1::PublicKey) -> Result<PublicKey> {
    let bytes = pk.to_bytes();
    PublicKey::from_sec1_bytes(&bytes)
        .map_err(|e| anyhow::anyhow!("converting gsigner public key: {e}"))
}

/// 20-byte Ethereum-style address from an ECDSA public key:
/// `keccak256(uncompressed_pubkey[1..])[12..]`.
pub fn address_bytes_from_public_key(pk: &PublicKey) -> [u8; 20] {
    // SEC1 uncompressed point: 0x04 || x(32) || y(32) — 65 bytes.
    let encoded = pk.inner().to_encoded_point(false);
    let bytes = encoded.as_bytes();
    debug_assert_eq!(bytes.len(), 65);
    let mut h = Keccak256::new();
    h.update(&bytes[1..]);
    let hash = h.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..]);
    out
}

/// Derive the libp2p secp256k1 secret used by the Malachite swarm
/// from the validator's master secret. Domain-separated so two
/// libp2p swarms under the same validator key (e.g. an application
/// network on QUIC plus the malachite TCP transport) don't collide
/// peer-ids.
pub fn derive_libp2p_secret(validator_secret: &[u8; 32]) -> [u8; 32] {
    const DOMAIN: &[u8] = b"mala-svc-libp2p:v1:";
    let mut h = Keccak256::new();
    h.update(DOMAIN);
    h.update(validator_secret);
    h.finalize().into()
}

/// Build the libp2p [`Keypair`] for the Malachite swarm. Zeroes the
/// transient derived bytes once they're inside the keypair.
pub fn libp2p_keypair_from(validator_secret: &[u8; 32]) -> Keypair {
    let mut derived = derive_libp2p_secret(validator_secret);
    let secret = libp2p_identity::secp256k1::SecretKey::try_from_bytes(&mut derived)
        .expect("derived libp2p secret is a valid secp256k1 scalar");
    for byte in derived.iter_mut() {
        *byte = 0;
    }
    let inner = libp2p_identity::secp256k1::Keypair::from(secret);
    Keypair::from(inner)
}

/// Compute the libp2p [`PeerId`] of the Malachite swarm associated
/// with `validator_secret` without spinning up the engine. Useful for
/// offline tooling: operators preparing `--persistent-peer` multiaddrs
/// can compute the `/p2p/<peer_id>` suffix from each validator's
/// keystore without booting the node.
pub fn libp2p_peer_id(validator_secret: &[u8; 32]) -> PeerId {
    libp2p_keypair_from(validator_secret).public().to_peer_id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_secret() -> impl Strategy<Value = [u8; 32]> {
        // Avoid the all-zero scalar (invalid on k256) by OR-ing a 1 in.
        any::<[u8; 32]>().prop_map(|mut s| {
            s[31] |= 1;
            s
        })
    }

    #[test]
    fn signer_round_trip() {
        let secret = [0x42u8; 32];
        let signer = MalachiteSigner::from_bytes(&secret).unwrap();
        let pk = signer.public_key();
        let sig = signer.sign(b"hello");
        assert!(signer.verify(b"hello", &sig, &pk));
        assert!(!signer.verify(b"goodbye", &sig, &pk));
    }

    #[test]
    fn libp2p_secret_is_domain_separated_and_deterministic() {
        let v = [0x77u8; 32];
        let l1 = derive_libp2p_secret(&v);
        let l2 = derive_libp2p_secret(&v);
        assert_eq!(l1, l2);
        assert_ne!(l1, v);
    }

    #[test]
    fn libp2p_secret_changes_per_validator() {
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        assert_ne!(derive_libp2p_secret(&a), derive_libp2p_secret(&b));
    }

    #[test]
    fn libp2p_peer_id_offline_matches_keypair() {
        let secret = [0x55u8; 32];
        let p1 = libp2p_peer_id(&secret);
        let p2 = libp2p_keypair_from(&secret).public().to_peer_id();
        assert_eq!(p1, p2);
    }

    #[test]
    fn address_is_20_bytes_and_deterministic() {
        let secret = [0x33u8; 32];
        let pk = private_key_from_bytes(&secret).unwrap().public_key();
        let a1 = address_bytes_from_public_key(&pk);
        let a2 = address_bytes_from_public_key(&pk);
        assert_eq!(a1, a2);
        assert_eq!(a1.len(), 20);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_sign_verify_round_trip(secret in arb_secret(), msg in proptest::collection::vec(any::<u8>(), 0..256)) {
            let signer = MalachiteSigner::from_bytes(&secret).unwrap();
            let pk = signer.public_key();
            let sig = signer.sign(&msg);
            prop_assert!(signer.verify(&msg, &sig, &pk));
        }

        #[test]
        fn prop_signature_rejects_tampered_message(
            secret in arb_secret(),
            msg in proptest::collection::vec(any::<u8>(), 1..64),
            tamper_idx in any::<u8>(),
        ) {
            let signer = MalachiteSigner::from_bytes(&secret).unwrap();
            let pk = signer.public_key();
            let sig = signer.sign(&msg);
            // Flip a byte to produce a definitely-different message.
            let mut tampered = msg.clone();
            let i = (tamper_idx as usize) % tampered.len();
            tampered[i] ^= 0xff;
            // It's possible (proptest may pick the original) that the
            // tampered message equals the original — guard for that.
            prop_assume!(tampered != msg);
            prop_assert!(!signer.verify(&tampered, &sig, &pk));
        }

        #[test]
        fn prop_libp2p_peer_id_is_pure_function(secret in arb_secret()) {
            prop_assert_eq!(libp2p_peer_id(&secret), libp2p_peer_id(&secret));
        }

        #[test]
        fn prop_distinct_secrets_yield_distinct_peer_ids(
            a in arb_secret(),
            b in arb_secret(),
        ) {
            prop_assume!(a != b);
            prop_assert_ne!(libp2p_peer_id(&a), libp2p_peer_id(&b));
        }
    }
}
