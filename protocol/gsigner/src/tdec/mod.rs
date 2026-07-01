// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Threshold-decryption key storage.
//!
//! This module stores validator threshold-decryption private material separately
//! from signing schemes. It intentionally does not implement [`crate::CryptoScheme`]:
//! these keys create decryption shares, not signatures.

pub type Bls12_381 = gear_tdec::bls12_381::E;
pub type TdecPublicKey = gear_tdec::keypair_common::PublicKey<Bls12_381>;
pub type TdecKeypair = gear_tdec::keypair_common::Keypair<Bls12_381>;
pub type TdecDecryptionKey = gear_tdec::DomainPoint<Bls12_381>;
pub type BlindedKeyShare = gear_tdec::BlindedKeyShare<Bls12_381>;
pub type PublicDecryptionContext = gear_tdec::PublicDecryptionContextSimple<Bls12_381>;

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub mod store;

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub use store::{TdecKeyEntry, TdecKeyStore};

#[cfg(all(test, feature = "std", feature = "keyring", feature = "serde"))]
mod tests;
