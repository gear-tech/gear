// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
    pub use crate::schemes::secp256k1::{
        self as primitives, Address, ContractSignature, Digest, FromActorIdError, MemoryStorage,
        PrivateKey, PublicKey, Secp256k1, Signature, SignedData, ToDigest,
    };
    #[cfg(feature = "std")]
    pub use crate::schemes::secp256k1::{FileStorage, Signer};
}

#[cfg(feature = "sr25519")]
pub mod sr25519 {
    pub use crate::{
        schemes::sr25519::{
            self as primitives, Keyring, Keystore, PrivateKey, PublicKey, Signature, Sr25519,
            Sr25519SignerExt,
        },
        substrate::SubstratePair,
    };
}
