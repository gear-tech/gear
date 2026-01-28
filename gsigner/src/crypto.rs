// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Convenient crypto re-exports mirroring the legacy `ethexe-common::crypto` module.
//!
//! These modules simply re-export the primitives provided by the scheme-specific
//! implementations under [`crate::schemes`], making it easier to migrate existing
//! imports and explore the available API surface.

#[cfg(feature = "secp256k1")]
pub mod secp256k1 {
    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub use crate::schemes::secp256k1::Signer;
    pub use crate::schemes::secp256k1::{
        self as primitives, Address, ContractSignature, Digest, FromActorIdError, PrivateKey,
        PublicKey, Secp256k1, Signature, SignedData, ToDigest,
    };
    #[cfg(all(feature = "serde", feature = "keyring"))]
    pub use crate::schemes::secp256k1::{Keyring, KeyringKeystore};
}

#[cfg(feature = "sr25519")]
pub mod sr25519 {
    #[cfg(all(feature = "serde", feature = "keyring"))]
    pub use crate::schemes::sr25519::Keyring;
    #[cfg(all(feature = "serde", feature = "std", feature = "keyring"))]
    pub use crate::schemes::sr25519::Keystore;
    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub use crate::schemes::sr25519::Sr25519SignerExt;
    pub use crate::{
        schemes::sr25519::{self as primitives, PrivateKey, PublicKey, Signature, Sr25519},
        substrate::SubstratePair,
    };
}

#[cfg(feature = "ed25519")]
pub mod ed25519 {
    pub use crate::schemes::ed25519::{
        self as primitives, Ed25519, PrivateKey, PublicKey, Signature,
    };
    #[cfg(all(feature = "serde", feature = "keyring"))]
    pub use crate::schemes::ed25519::{Keyring, KeyringKeystore};
}
