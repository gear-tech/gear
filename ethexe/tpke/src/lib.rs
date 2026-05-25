// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Threshold public-key encryption for ethexe private transactions.
//!
//! Construction: Boneh-Franklin identity-based TPKE on BLS12-381, with the
//! master secret split into n Shamir shares (threshold t). Encryption is
//! identity-bound: every ciphertext carries an `id` and a decryption share
//! produced for `id` only decrypts that one ciphertext.
//!
//! Pairing orientation (Type-3 on BLS12-381):
//!   - `Q_id ∈ G1` via hash-to-curve (DST below)
//!   - master pubkey, share pubkeys, ephemeral U  ∈ G2
//!   - decryption shares                          ∈ G1
//!   - e: G1 × G2 → GT
//!
//! IND-CCA via ChaCha20-Poly1305 (KEM/DEM with HKDF-SHA256 key/nonce derivation).
//! The DEM AAD binds (id, U_bytes, chain_id, key_epoch_id) into the MAC.

// !!!!!!!!!!!!!!!!!
// !!!!!!!!!!!!!!!!!
// !!!!!!!!!!!!!!!!!
// !!!!!!!!!!!!!!!!!
// TODO: also can doc for this crate using #[doc = include_str!("algorithm_doc.md")]

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

/// TPKE encrypt/decrypt implementation.
mod tpke;
pub use tpke::{HKDF_DEM_INFO, decrypt, encrypt};

/// Shamir's Secret-Share-Splitting implementation.
mod dealer;
pub use dealer::deal;

/// TPKE primitive types.
mod primitives;
pub use primitives::{
    Blake2b256Hash, Ciphertext, DealerOutput, DecryptionShare, Encrypted, MasterPublicKey,
    MasterSecretKey, SecretKeyShare, SharePublicKey,
};

mod utils;
pub use utils::HTC_DOMAIN;

// Re-export random traits to use them without ark dependencies.
pub mod rand {
    pub use ark_std::rand::{CryptoRng, RngCore};
}

#[cfg(all(test, feature = "std"))]
mod tests;

/// TPKE error type.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum TpkeError {
    #[error("malformed ciphertext envelope")]
    MalformedCiphertext,
    #[error("aead error")]
    Aead,
    #[error("decryption share did not verify against share public key")]
    ShareVerification,
    #[error("not enough shares to combine: got {got}, need {need}")]
    InsufficientShares { got: usize, need: usize },
    #[error("duplicate share index {0}")]
    DuplicateShareIndex(u32),
    #[error("share index can not be zero")]
    ZeroShareIndex,
    #[error("share #{index} bound to a different envelope id than the target")]
    ShareEnvelopeMismatch { index: u32 },
    #[error("point serialization failed")]
    Serialization,
    #[error("hash-to-curve failed")]
    HashToCurve,
    #[error("invalid threshold: t={t}, n={n} (require 1 <= t <= n)")]
    InvalidThreshold { t: u32, n: u32 },
    #[error("public key is the identity point — refusing to use it")]
    IdentityPublicKey,
    #[error("payload decode failed: {0}")]
    PayloadDecode(parity_scale_codec::Error),
}

pub type Result<T> = core::result::Result<T, TpkeError>;
