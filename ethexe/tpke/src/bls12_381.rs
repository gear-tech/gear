// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::{Bls12_381, Fr, G1Affine, G1Projective, G2Affine};
use ark_ec::{
    AffineRepr, CurveGroup,
    hashing::{HashToCurve, curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::Pairing,
};
use ark_ff::{UniformRand, Zero, field_hashers::DefaultFieldHasher};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{CryptoRng, RngCore};
use blake2::{Blake2b, Digest, digest::consts::U32};
use sha2::Sha256;

use crate::{
    DecryptionShare, EncryptedEnvelope, MasterPublicKey, SecretKeyShare, SharePublicKey, TpkeError,
    aead, shamir::lagrange_coefficient,
};

/// Hash-to-curve domain separation tag, version-locked. Changing this string
/// invalidates every in-flight ciphertext — do not modify post-launch.
pub const DST_G1: &[u8] = b"ETHEXE-TPKE-V1-BLS12381G1_XMD:SHA-256_SSWU_RO_";

/// Blake2b domain tag for `id` derivation.
pub const ID_DOMAIN: &[u8] = b"ethexe-tpke-v1";

/// Compressed G2 point byte length on BLS12-381.
pub const G2_COMPRESSED_LEN: usize = 96;
/// Compressed G1 point byte length on BLS12-381.
pub const G1_COMPRESSED_LEN: usize = 48;

type Blake2b256 = Blake2b<U32>;
type G1Hasher = MapToCurveBasedHasher<
    G1Projective,
    DefaultFieldHasher<Sha256, 128>,
    WBMap<ark_bls12_381::g1::Config>,
>;

/// Derive the 32-byte identity that binds a ciphertext to its plaintext,
/// chain, and key epoch.
///
/// `user_nonce` MUST be high-entropy randomness chosen at encryption time.
/// Without it, an attacker who can guess plaintext (e.g. known token-trade
/// templates) can verify the guess by recomputing `id` and matching the
/// ciphertext's id — a known-plaintext attack on the identity.
pub fn derive_id(
    chain_id: u64,
    key_epoch_id: u32,
    canonical_plaintext: &[u8],
    user_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut h = Blake2b256::new();
    h.update(ID_DOMAIN);
    h.update(chain_id.to_le_bytes());
    h.update(key_epoch_id.to_le_bytes());
    h.update(canonical_plaintext);
    h.update(user_nonce);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    out
}

// The hasher is constructed once per process and reused. DST_G1 is constant,
// so `G1Hasher::new` only fails for malformed DSTs — which we control at
// compile time — making the `.expect()` unreachable in practice.
#[cfg(feature = "std")]
fn g1_hasher() -> &'static G1Hasher {
    use std::sync::OnceLock;
    static HASHER: OnceLock<G1Hasher> = OnceLock::new();
    HASHER.get_or_init(|| G1Hasher::new(DST_G1).expect("DST_G1 is a valid hash-to-curve DST"))
}

fn hash_to_g1(id: &[u8; 32]) -> Result<G1Affine, TpkeError> {
    #[cfg(feature = "std")]
    {
        g1_hasher().hash(id).map_err(|_| TpkeError::HashToCurve)
    }
    #[cfg(not(feature = "std"))]
    {
        let hasher = G1Hasher::new(DST_G1).map_err(|_| TpkeError::HashToCurve)?;
        hasher.hash(id).map_err(|_| TpkeError::HashToCurve)
    }
}

/// Serialize an arkworks point/element to its fixed-size compressed bytes.
pub(crate) fn serialize_compressed<P: CanonicalSerialize, const N: usize>(
    p: &P,
) -> Result<[u8; N], TpkeError> {
    let mut buf = [0u8; N];
    p.serialize_compressed(&mut buf[..])
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Deserialize a fixed-size compressed-bytes blob into an arkworks point.
pub(crate) fn deserialize_compressed<P: CanonicalDeserialize, const N: usize>(
    bytes: &[u8; N],
) -> Result<P, TpkeError> {
    P::deserialize_compressed(&bytes[..]).map_err(|_| TpkeError::MalformedCiphertext)
}

pub(crate) fn serialize_g2(p: &G2Affine) -> Result<[u8; G2_COMPRESSED_LEN], TpkeError> {
    serialize_compressed::<_, G2_COMPRESSED_LEN>(p)
}

fn deserialize_g2(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<G2Affine, TpkeError> {
    deserialize_compressed::<G2Affine, G2_COMPRESSED_LEN>(bytes)
}

pub(crate) fn serialize_g1(p: &G1Affine) -> Result<[u8; G1_COMPRESSED_LEN], TpkeError> {
    serialize_compressed::<_, G1_COMPRESSED_LEN>(p)
}

/// Encrypt `plaintext` for identity `id` under master public key `pk`.
///
/// `chain_id` and `key_epoch_id` are bound into the AEAD's AAD so a ciphertext
/// can only be decrypted within its intended chain+epoch context.
pub fn encrypt<R: RngCore + CryptoRng>(
    pk: &MasterPublicKey,
    id: &[u8; 32],
    chain_id: u64,
    key_epoch_id: u32,
    plaintext: &[u8],
    rng: &mut R,
) -> Result<EncryptedEnvelope, TpkeError> {
    // Identity public key would make z = e(Q_id, 0) = 1_GT, derivable by anyone
    // who sees the ciphertext — confidentiality fully bypassed. Reject.
    if pk.0.is_zero() {
        return Err(TpkeError::IdentityPublicKey);
    }

    let q_id = hash_to_g1(id)?;
    // Reject the (negligibly likely) malformed id that hashes to the identity.
    if q_id.is_zero() {
        return Err(TpkeError::HashToCurve);
    }

    let u_scalar = Fr::rand(rng);
    let u_point = (G2Affine::generator() * u_scalar).into_affine();
    let u_bytes = serialize_g2(&u_point)?;

    // z = e(Q_id, AggPub)^u = e(Q_id, g₂)^(S·u)
    let z_base = Bls12_381::pairing(q_id, pk.0);
    let z = z_base * u_scalar;
    let body = aead::encrypt_body(&z, id, &u_bytes, chain_id, key_epoch_id, plaintext)?;

    Ok(EncryptedEnvelope {
        u: u_bytes,
        id: *id,
        body,
    })
}

impl SecretKeyShare {
    /// Validator-side: produce `Dᵢ = Sᵢ · Q_id` for the ciphertext's id.
    pub fn decrypt_share(
        &self,
        envelope: &EncryptedEnvelope,
    ) -> Result<DecryptionShare, TpkeError> {
        if self.index == 0 {
            return Err(TpkeError::ZeroShareIndex(0));
        }
        let q_id = hash_to_g1(&envelope.id)?;
        let point = (q_id * self.scalar()).into_affine();
        Ok(DecryptionShare {
            index: self.index,
            id: envelope.id,
            point,
        })
    }
}

impl SharePublicKey {
    /// Verify a decryption share: e(Dᵢ, g₂) ?= e(Q_id, PSᵢ).
    ///
    /// Returns `Ok(false)` when the share's validator index or envelope id
    /// doesn't match what we're verifying against.
    pub fn verify(
        &self,
        envelope: &EncryptedEnvelope,
        share: &DecryptionShare,
    ) -> Result<bool, TpkeError> {
        if share.index != self.index || share.id != envelope.id {
            return Ok(false);
        }
        let q_id = hash_to_g1(&envelope.id)?;
        let g2 = G2Affine::generator();
        let lhs = Bls12_381::pairing(share.point, g2);
        let rhs = Bls12_381::pairing(q_id, self.point);
        Ok(lhs == rhs)
    }
}

/// Combine `t` decryption shares into the plaintext.
///
/// This function does NOT verify shares cryptographically — callers must run
/// `SharePublicKey::verify` on each share they trust as honest. It DOES enforce:
///   - share count ≥ threshold
///   - every share's `id` matches `envelope.id`
///   - no zero or duplicate validator indices
///
/// **Only the first `threshold` shares from `shares` are consumed.** Excess
/// shares are ignored. To use a specific subset, slice the input yourself.
pub fn combine(
    envelope: &EncryptedEnvelope,
    shares: &[DecryptionShare],
    chain_id: u64,
    key_epoch_id: u32,
    threshold: u32,
) -> Result<Vec<u8>, TpkeError> {
    // Match deal()'s invariant: threshold 0 would short-circuit to the G1
    // identity for D and let anyone "decrypt" ciphertexts produced under an
    // identity master pubkey. Reject up front.
    if threshold == 0 {
        return Err(TpkeError::InvalidThreshold { t: 0, n: 0 });
    }
    if (shares.len() as u32) < threshold {
        return Err(TpkeError::InsufficientShares {
            got: shares.len(),
            need: threshold as usize,
        });
    }
    // Use the first `threshold` shares.
    let used = &shares[..threshold as usize];

    // Reject zero/duplicate indices and envelope mismatches.
    let mut seen: Vec<u32> = Vec::with_capacity(used.len());
    for s in used {
        if s.index == 0 {
            return Err(TpkeError::ZeroShareIndex(0));
        }
        if s.id != envelope.id {
            return Err(TpkeError::ShareEnvelopeMismatch { index: s.index });
        }
        if seen.contains(&s.index) {
            return Err(TpkeError::DuplicateShareIndex(s.index));
        }
        seen.push(s.index);
    }

    // D = Σ λᵢ · Dᵢ (Lagrange in the exponent)
    let mut acc = G1Projective::zero();
    for s in used {
        let lambda =
            lagrange_coefficient(s.index, &seen).ok_or(TpkeError::DuplicateShareIndex(s.index))?;
        acc += s.point * lambda;
    }
    let d = acc.into_affine();

    // z' = e(D, U)
    let u_point = deserialize_g2(&envelope.u)?;
    let z = Bls12_381::pairing(d, u_point);

    aead::decrypt_body(
        &z,
        &envelope.id,
        &envelope.u,
        chain_id,
        key_epoch_id,
        &envelope.body,
    )
}

impl MasterPublicKey {
    pub fn to_bytes(&self) -> Result<[u8; G2_COMPRESSED_LEN], TpkeError> {
        serialize_g2(&self.0)
    }

    /// Deserialize the master pubkey, rejecting the G2 identity point. An
    /// identity-element master pubkey would make `e(Q_id, pk) = 1_GT`, letting
    /// anyone with the ciphertext derive the DEM key without shares.
    pub fn from_bytes(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self, TpkeError> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self(point))
    }
}

impl SharePublicKey {
    pub fn to_bytes(&self) -> Result<(u32, [u8; G2_COMPRESSED_LEN]), TpkeError> {
        Ok((self.index, serialize_g2(&self.point)?))
    }

    /// Deserialize a share pubkey, rejecting the G2 identity point. An identity
    /// share-pubkey would make share verification accept any honest share for
    /// that index regardless of the underlying secret-share scalar.
    pub fn from_bytes(index: u32, bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self, TpkeError> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self { index, point })
    }
}
