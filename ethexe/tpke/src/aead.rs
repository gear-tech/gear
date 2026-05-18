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
use parity_scale_codec::{Decode, Encode};
use sha2::Sha256;

use crate::{Encryptable, TpkeError, TpkeResult, keys::Ciphertext};

/// HKDF info prefix for the KEM-derived DEM key/nonce.
pub const HKDF_DEM_INFO: &[u8] = b"ethexe-tpke-dem-v1";

/// 32-byte ChaCha20-Poly1305 key length.
const DEM_KEY_LEN: usize = 32;
/// 12-byte ChaCha20-Poly1305 nonce length.
const DEM_NONCE_LEN: usize = 12;

/// Additional Authenticated Data layout (input to ChaCha20-Poly1305 MAC).
///
/// Format: id ‖ U_bytes ‖ chain_id_le ‖ key_epoch_id_le
fn build_aad(id: impl AsRef<[u8]>, u_bytes: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(id.as_ref().len() + u_bytes.len());
    aad.extend_from_slice(id.as_ref());
    aad.extend_from_slice(u_bytes);
    aad
}

/// Encode the pairing-target element `z ∈ GT` deterministically for HKDF input.
fn serialize_gt(z: &PairingOutput<Bls12_381>) -> TpkeResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(576);
    z.serialize_compressed(&mut buf)
        .map_err(|_| TpkeError::Serialization)?;
    Ok(buf)
}

/// Derive the 44 raw bytes (32-byte AEAD key + 12-byte AEAD nonce) from the
/// shared secret `z`, identity, and ephemeral `U`.
fn derive_dem_key_nonce(
    z: &PairingOutput<Bls12_381>,
    id: impl AsRef<[u8]>,
    u_bytes: &[u8],
) -> TpkeResult<([u8; DEM_KEY_LEN], [u8; DEM_NONCE_LEN])> {
    let z_bytes = serialize_gt(z)?;
    let mut info = Vec::with_capacity(HKDF_DEM_INFO.len() + 32 + u_bytes.len());
    info.extend_from_slice(HKDF_DEM_INFO);
    info.extend_from_slice(id.as_ref());
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

pub(crate) fn encrypt_payload<T: Encryptable>(
    z: &PairingOutput<Bls12_381>,
    id: &T::Id,
    u_bytes: &[u8],
    payload: &T::Payload,
) -> TpkeResult<Ciphertext<T::Payload>> {
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, id, u_bytes)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(&id, u_bytes);
    let msg = payload.encode();
    cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: msg.as_ref(),
                aad: &aad,
            },
        )
        .map(|inner| Ciphertext::new(inner))
        .map_err(|_| TpkeError::AeadAuth)
}

pub(crate) fn decrypt_payload<T: Encryptable>(
    z: &PairingOutput<Bls12_381>,
    envelope_id: &T::Id,
    u_bytes: &[u8],
    ciphertext: &Ciphertext<T::Payload>,
) -> TpkeResult<T::Payload> {
    let (key_bytes, nonce_bytes) = derive_dem_key_nonce(z, envelope_id, u_bytes)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let aad = build_aad(envelope_id, u_bytes);
    let data = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: ciphertext.as_ref(),
                aad: &aad,
            },
        )
        .map_err(|_| TpkeError::AeadAuth)?;
    <T::Payload as Decode>::decode(&mut data.as_slice()).map_err(TpkeError::PayloadDecode)
}
