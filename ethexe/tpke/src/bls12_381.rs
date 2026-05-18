// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ark_bls12_381::{Bls12_381, G1Affine, G1Projective, G2Affine};
use ark_ec::{
    AffineRepr, CurveGroup,
    hashing::{HashToCurve, curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::Pairing,
};
use ark_ff::{Zero, field_hashers::DefaultFieldHasher};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use sha2::Sha256;

use crate::{
    DecryptionShare, Encryptable, Encrypted, MasterPublicKey, SecretKeyShare, SharePublicKey,
    TpkeError, TpkeResult, aead, shamir::lagrange_coefficient,
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

type G1Hasher = MapToCurveBasedHasher<
    G1Projective,
    DefaultFieldHasher<Sha256, 128>,
    WBMap<ark_bls12_381::g1::Config>,
>;

pub(crate) fn hash_to_g1(id: impl AsRef<[u8]>) -> TpkeResult<G1Affine> {
    let hasher = G1Hasher::new(DST_G1).map_err(|_| TpkeError::HashToCurve)?;
    hasher.hash(id.as_ref()).map_err(|_| TpkeError::HashToCurve)
}

/// Serialize an arkworks point/element to its fixed-size compressed bytes.
pub(crate) fn serialize_compressed<P: CanonicalSerialize, const N: usize>(
    p: &P,
) -> TpkeResult<[u8; N]> {
    let mut buf = [0u8; N];
    p.serialize_compressed(&mut buf[..])
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Deserialize a fixed-size compressed-bytes blob into an arkworks point.
pub(crate) fn deserialize_compressed<P: CanonicalDeserialize, const N: usize>(
    bytes: &[u8; N],
) -> TpkeResult<P> {
    P::deserialize_compressed(&bytes[..]).map_err(|_| TpkeError::MalformedCiphertext)
}

pub(crate) fn serialize_g2(p: &G2Affine) -> TpkeResult<[u8; G2_COMPRESSED_LEN]> {
    serialize_compressed::<_, G2_COMPRESSED_LEN>(p)
}

fn deserialize_g2(bytes: &[u8; G2_COMPRESSED_LEN]) -> TpkeResult<G2Affine> {
    deserialize_compressed::<G2Affine, G2_COMPRESSED_LEN>(bytes)
}

pub(crate) fn serialize_g1(p: &G1Affine) -> TpkeResult<[u8; G1_COMPRESSED_LEN]> {
    serialize_compressed::<_, G1_COMPRESSED_LEN>(p)
}

impl SecretKeyShare {
    /// Validator-side: produce `Dᵢ = Sᵢ · Q_id` for the ciphertext's id.
    pub fn decrypt_share<T>(&self, encrypted: &Encrypted<T>) -> TpkeResult<DecryptionShare<T>>
    where
        T: Encryptable,
    {
        if self.index == 0 {
            return Err(TpkeError::ZeroShareIndex(0));
        }
        let q_id = hash_to_g1(&encrypted.id)?;
        let point = (q_id * self.scalar()).into_affine();
        Ok(DecryptionShare {
            index: self.index,
            id: encrypted.id,
            point,
        })
    }
}

impl SharePublicKey {
    /// Verify a decryption share: e(Dᵢ, g₂) ?= e(Q_id, PSᵢ).
    ///
    /// Returns `Ok(false)` when the share's validator index or envelope id
    /// doesn't match what we're verifying against.
    pub fn verify<T>(&self, envelope: &Encrypted<T>, share: &DecryptionShare<T>) -> TpkeResult<bool>
    where
        T: Encryptable,
    {
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
pub fn combine<T>(
    envelope: &Encrypted<T>,
    shares: &[DecryptionShare<T>],
    threshold: u32,
) -> TpkeResult<T::Payload>
where
    T: Encryptable,
{
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

    aead::decrypt_payload::<T>(&z, &envelope.id, &envelope.u, &envelope.ciphertext)
}

impl MasterPublicKey {
    pub fn to_bytes(&self) -> TpkeResult<[u8; G2_COMPRESSED_LEN]> {
        serialize_g2(&self.0)
    }

    /// Deserialize the master pubkey, rejecting the G2 identity point. An
    /// identity-element master pubkey would make `e(Q_id, pk) = 1_GT`, letting
    /// anyone with the ciphertext derive the DEM key without shares.
    pub fn from_bytes(bytes: &[u8; G2_COMPRESSED_LEN]) -> TpkeResult<Self> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self(point))
    }
}

impl SharePublicKey {
    pub fn to_bytes(&self) -> TpkeResult<(u32, [u8; G2_COMPRESSED_LEN])> {
        Ok((self.index, serialize_g2(&self.point)?))
    }

    /// Deserialize a share pubkey, rejecting the G2 identity point. An identity
    /// share-pubkey would make share verification accept any honest share for
    /// that index regardless of the underlying secret-share scalar.
    pub fn from_bytes(index: u32, bytes: &[u8; G2_COMPRESSED_LEN]) -> TpkeResult<Self> {
        let point = deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self { index, point })
    }
}
