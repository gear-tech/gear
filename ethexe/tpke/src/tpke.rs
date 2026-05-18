// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::{Bls12_381, Fr, G1Projective, G2Affine};
use ark_ec::{
    AffineRepr, CurveGroup,
    pairing::{Pairing, PairingOutput},
};
use ark_ff::Zero;
use ark_std::{
    UniformRand,
    rand::{CryptoRng, RngCore},
};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use parity_scale_codec::{Decode, Encode};
use sha2::Sha256;

use crate::{
    Blake2b256Hash, Ciphertext, DecryptionShare, Encrypted, MasterPublicKey, Result, TpkeError,
    shamir::lagrange_coefficient, utils,
};

/// HKDF info prefix for the KEM-derived DEM key/nonce.
pub const HKDF_DEM_INFO: &[u8] = b"ethexe-tpke-dem-v1";

/// 32-byte ChaCha20-Poly1305 key length.
const DEM_KEY_LEN: usize = 32;
/// 12-byte ChaCha20-Poly1305 nonce length.
const DEM_NONCE_LEN: usize = 12;

pub fn encrypt<P, R>(payload: &P, pk: &MasterPublicKey, rng: &mut R) -> Result<Encrypted<P>>
where
    R: RngCore + CryptoRng,
    P: Encode,
{
    // Identity public key would make z = e(Q_id, 0) = 1_GT, derivable by anyone
    // who sees the ciphertext — confidentiality fully bypassed. Reject.
    if pk.0.is_zero() {
        return Err(TpkeError::IdentityPublicKey);
    }

    let hash = Blake2b256Hash::from(payload);
    let q = utils::hash_to_g1(hash)?;

    // Reject the (negligibly likely) malformed id that hashes to the identity.
    if q.is_zero() {
        return Err(TpkeError::HashToCurve);
    }

    let u_scalar = Fr::rand(rng);
    let u_point = (G2Affine::generator() * u_scalar).into_affine();
    let u_bytes = utils::serialize_g2(&u_point)?;

    // z = e(Q_id, AggPub)^u = e(Q_id, g₂)^(S·u)
    let z = Bls12_381::pairing(q, pk.0) * u_scalar;
    let ciphertext = encrypt_payload::<P>(&z, &hash, &u_bytes, payload)?;

    Ok(Encrypted {
        u: u_bytes,
        hash,
        ciphertext,
    })
}

pub fn decrypt<P>(encrypted: &Encrypted<P>, shares: &[DecryptionShare<P>]) -> Result<P>
where
    P: Encode + Decode,
{
    let mut seen_indexes = Vec::with_capacity(shares.len());
    for share in shares {
        if share.index == 0 {
            return Err(TpkeError::ZeroShareIndex);
        }
        if share.hash != encrypted.hash {
            return Err(TpkeError::ShareEnvelopeMismatch { index: share.index });
        }
        if seen_indexes.contains(&share.index) {
            return Err(TpkeError::DuplicateShareIndex(share.index));
        }
        seen_indexes.push(share.index);
    }

    // D = sum(λᵢ * Dᵢ) (Lagrange interpolation)
    let mut restored_secret = G1Projective::zero();
    for share in shares {
        let lambda = lagrange_coefficient(share.index, &seen_indexes)
            .ok_or(TpkeError::DuplicateShareIndex(share.index))?;
        restored_secret += share.point * lambda;
    }
    let d = restored_secret.into_affine();

    // z' = e(D, U)
    let u_point = utils::deserialize_g2(&encrypted.u)?;
    let z = Bls12_381::pairing(d, u_point);

    decrypt_payload::<P>(&z, &encrypted.hash, &encrypted.u, &encrypted.ciphertext)
}

/// Additional Authenticated Data layout (input to ChaCha20-Poly1305).
/// Format: hash ‖ U_bytes
fn build_aad(hash: impl AsRef<[u8]>, u_bytes: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(hash.as_ref().len() + u_bytes.len());
    aad.extend_from_slice(hash.as_ref());
    aad.extend_from_slice(u_bytes);
    aad
}

/// Encode the pairing-target element `z ∈ GT` deterministically for HKDF input.
/// Derive the 44 raw bytes (32-byte AEAD key + 12-byte AEAD nonce) from the
/// shared secret `z`, identity, and ephemeral `U`.
fn derive_dem_key_nonce(
    z: &PairingOutput<Bls12_381>,
    hash: impl AsRef<[u8]>,
    u_bytes: &[u8],
) -> Result<([u8; DEM_KEY_LEN], [u8; DEM_NONCE_LEN])> {
    let z_bytes = utils::serialize_gt(z)?;
    let mut info = Vec::with_capacity(HKDF_DEM_INFO.len() + 32 + u_bytes.len());
    info.extend_from_slice(HKDF_DEM_INFO);
    info.extend_from_slice(hash.as_ref());
    info.extend_from_slice(u_bytes);

    let hk = Hkdf::<Sha256>::new(None, &z_bytes);
    let mut okm = [0u8; DEM_KEY_LEN + DEM_NONCE_LEN];
    hk.expand(&info, &mut okm)
        .map_err(|_| TpkeError::Serialization)?;

    let mut key = [0u8; DEM_KEY_LEN];
    let mut nonce = [0u8; DEM_NONCE_LEN];
    key.copy_from_slice(&okm[..DEM_KEY_LEN]);
    nonce.copy_from_slice(&okm[DEM_KEY_LEN..]);
    Ok((key, nonce))
}

fn encrypt_payload<P: Encode>(
    z: &PairingOutput<Bls12_381>,
    hash: &Blake2b256Hash<P>,
    u_bytes: &[u8],
    payload: &P,
) -> Result<Ciphertext<P>> {
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, hash, u_bytes)?;

    let encoded_payload = payload.encode();
    let aad = build_aad(hash, u_bytes);

    let encrypted_bytes = ChaCha20Poly1305::new(Key::from_slice(&key_bytes)).encrypt(
        Nonce::from_slice(&nonce_bytes),
        Payload {
            msg: encoded_payload.as_ref(),
            aad: &aad,
        },
    )?;

    Ok(Ciphertext::new(encrypted_bytes))
}

fn decrypt_payload<P: Decode>(
    z: &PairingOutput<Bls12_381>,
    hash: &Blake2b256Hash<P>,
    u_bytes: &[u8],
    ciphertext: &Ciphertext<P>,
) -> Result<P> {
    // TODO: add here check that data is correctly decoded (id matches to real data).
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, hash, u_bytes)?;
    let aad = build_aad(hash, u_bytes);

    let decrypted_bytes = ChaCha20Poly1305::new(Key::from_slice(&key_bytes)).decrypt(
        Nonce::from_slice(&nonce_bytes),
        Payload {
            msg: ciphertext.as_ref(),
            aad: &aad,
        },
    )?;

    let decoded_payload =
        P::decode(&mut decrypted_bytes.as_slice()).map_err(TpkeError::PayloadDecode)?;

    // if Blake2b256Hash::from(&decoded_payload) != *hash {
    //     todo!("return error here, because the payload mismatch")
    // }

    Ok(decoded_payload)
}
