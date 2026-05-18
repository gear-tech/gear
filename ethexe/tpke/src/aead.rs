// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::Bls12_381;
use ark_ec::pairing::PairingOutput;
use ark_serialize::CanonicalSerialize;
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::TpkeError;

/// HKDF info prefix for the KEM-derived DEM key/nonce.
pub const HKDF_DEM_INFO: &[u8] = b"ethexe-tpke-dem-v1";

/// 32-byte ChaCha20-Poly1305 key length.
const DEM_KEY_LEN: usize = 32;
/// 12-byte ChaCha20-Poly1305 nonce length.
const DEM_NONCE_LEN: usize = 12;

/// Additional Authenticated Data layout (input to ChaCha20-Poly1305 MAC).
///
/// Format: id ‖ U_bytes ‖ chain_id_le ‖ key_epoch_id_le
fn build_aad(envelope_id: &[u8; 32], u_bytes: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + u_bytes.len());
    aad.extend_from_slice(envelope_id);
    aad.extend_from_slice(u_bytes);
    aad
}

/// Encode the pairing-target element `z ∈ GT` deterministically for HKDF input.
fn serialize_gt(z: &PairingOutput<Bls12_381>) -> Result<Vec<u8>, TpkeError> {
    let mut buf = Vec::with_capacity(576);
    z.serialize_compressed(&mut buf)
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Derive the 44 raw bytes (32-byte AEAD key + 12-byte AEAD nonce) from the
/// shared secret `z`, identity, and ephemeral `U`.
fn derive_dem_key_nonce(
    z: &PairingOutput<Bls12_381>,
    envelope_id: &[u8; 32],
    u_bytes: &[u8],
) -> Result<([u8; DEM_KEY_LEN], [u8; DEM_NONCE_LEN]), TpkeError> {
    let z_bytes = serialize_gt(z)?;
    let mut info = Vec::with_capacity(HKDF_DEM_INFO.len() + 32 + u_bytes.len());
    info.extend_from_slice(HKDF_DEM_INFO);
    info.extend_from_slice(envelope_id);
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

pub(crate) fn encrypt_body(
    z: &PairingOutput<Bls12_381>,
    envelope_id: &[u8; 32],
    u_bytes: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, TpkeError> {
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, envelope_id, u_bytes)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(envelope_id, u_bytes);
    cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| TpkeError::AeadAuth)
}

pub(crate) fn decrypt_body(
    z: &PairingOutput<Bls12_381>,
    envelope_id: &[u8; 32],
    u_bytes: &[u8],
    chain_id: u64,
    key_epoch_id: u32,
    body: &[u8],
) -> Result<Vec<u8>, TpkeError> {
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, envelope_id, u_bytes)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(envelope_id, u_bytes, chain_id, key_epoch_id);
    cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: body,
                aad: &aad,
            },
        )
        .map_err(|_| TpkeError::AeadAuth)
}
