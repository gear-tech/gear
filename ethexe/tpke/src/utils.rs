// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Result, TpkeError};
use ark_bls12_381::{Bls12_381, G1Affine, G1Projective, G2Affine};
use ark_ec::{
    hashing::{HashToCurve, curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::PairingOutput,
};
use ark_ff::field_hashers::DefaultFieldHasher;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use sha2::Sha256;

/// Hash-to-curve domain separation tag, version-locked. Changing this string
/// invalidates every in-flight ciphertext — do not modify post-launch.
pub const HTC_DOMAIN: &[u8] = b"ETHEXE-TPKE-V1-BLS12381G1_XMD:SHA-256_SSWU_RO_";

/// Compressed G1 point byte length on BLS12-381.
pub(crate) const G1_COMPRESSED_LEN: usize = size_of::<G1Affine>();
/// Compressed G2 point byte length on BLS12-381.
pub(crate) const G2_COMPRESSED_LEN: usize = size_of::<G2Affine>();
/// Compressed GT point byte length on BLS12-381.
pub(crate) const GT_COMPRESSED_LEN: usize = size_of::<PairingOutput<Bls12_381>>();

pub(crate) fn hash_to_g1(hash: impl AsRef<[u8]>) -> Result<G1Affine> {
    let hasher = MapToCurveBasedHasher::<
        G1Projective,
        DefaultFieldHasher<Sha256, 128>,
        WBMap<ark_bls12_381::g1::Config>,
    >::new(HTC_DOMAIN)
    .map_err(|_| TpkeError::HashToCurve)?;

    hasher
        .hash(hash.as_ref())
        .map_err(|_| TpkeError::HashToCurve)
}

/// Serialize an arkworks point/element to its fixed-size compressed bytes.
pub(crate) fn serialize_compressed<P: CanonicalSerialize, const N: usize>(
    p: &P,
) -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    p.serialize_compressed(&mut buf[..])
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Deserialize a fixed-size compressed-bytes blob into an arkworks point.
pub(crate) fn deserialize_compressed<P: CanonicalDeserialize, const N: usize>(
    bytes: &[u8; N],
) -> Result<P> {
    P::deserialize_compressed(&bytes[..]).map_err(|_| TpkeError::MalformedCiphertext)
}

pub(crate) fn serialize_g2(p: &G2Affine) -> Result<[u8; G2_COMPRESSED_LEN]> {
    serialize_compressed::<_, G2_COMPRESSED_LEN>(p)
}

pub(crate) fn deserialize_g2(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<G2Affine> {
    deserialize_compressed::<G2Affine, G2_COMPRESSED_LEN>(bytes)
}

pub(crate) fn serialize_g1(p: &G1Affine) -> Result<[u8; G1_COMPRESSED_LEN]> {
    serialize_compressed::<_, G1_COMPRESSED_LEN>(p)
}

pub(crate) fn serialize_gt(z: &PairingOutput<Bls12_381>) -> Result<[u8; GT_COMPRESSED_LEN]> {
    serialize_compressed::<_, GT_COMPRESSED_LEN>(z)
}
