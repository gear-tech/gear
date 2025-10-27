#![cfg_attr(not(feature = "std"), no_std)]
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

//! Universal cryptographic signer library supporting multiple signature schemes.
//!
//! This crate provides a unified interface for cryptographic signing operations
//! supporting both secp256k1 (Ethereum) and sr25519 (Substrate) signature schemes.
//!
//! # Features
//!
//! - `secp256k1` - Enable Ethereum/secp256k1 ECDSA support (enabled by default)
//! - `sr25519` - Enable Substrate/sr25519 Schnorrkel support (enabled by default)
//! - `cli` - Enable command-line interface tools
//! - `codec` - Enable parity-scale-codec support for serialization
//! - `keyring` - Keyring support with primary key management
//! - `gprimitives` - Enable gprimitives integration (for ActorId conversions)
//! - `alloy-primitives` - Enable alloy-primitives integration
//! - `sp-core` - Enable sp-core integration (Substrate compatibility)
//! - `sp-runtime` - Enable sp-runtime integration (Substrate compatibility)
//!
//! # Examples
//!
//! ```rust,ignore
//! use gsigner::secp256k1;
//!
//! // Create an in-memory signer
//! let signer = secp256k1::Signer::memory();
//!
//! // Generate a new key
//! let public_key = signer.generate_key()?;
//!
//! // Sign some data
//! let signature = signer.sign(public_key, b"hello world")?;
//! ```

extern crate alloc;

#[cfg(all(not(feature = "std"), feature = "sr25519"))]
compile_error!("The `sr25519` feature requires the `std` feature.");

pub mod address;
pub mod crypto;
pub mod error;
pub mod schemes;
#[cfg(feature = "std")]
pub mod signer;
pub mod storage;
pub mod traits;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "keyring")]
pub mod keyring;

#[cfg(feature = "sr25519")]
pub mod substrate;

#[cfg(feature = "secp256k1")]
pub use address::Address;
#[cfg(feature = "secp256k1")]
pub use address::FromActorIdError;
#[cfg(feature = "sr25519")]
pub use address::SubstrateAddress;
pub use error::{Result, SignerError};
#[cfg(feature = "std")]
pub use signer::Signer;
#[cfg(feature = "sr25519")]
pub use substrate::SubstratePair;
pub use traits::{KeyStorage, SignatureScheme};

#[cfg(feature = "secp256k1")]
pub use schemes::secp256k1::{
    ContractSignature, Digest, PrivateKey, PublicKey, Signature, SignedData, ToDigest, VerifiedData,
};

#[cfg(all(feature = "secp256k1", feature = "std"))]
pub use storage::FSKeyStorage;
#[cfg(feature = "secp256k1")]
pub use storage::MemoryKeyStorage;

#[cfg(feature = "secp256k1")]
pub mod secp256k1 {
    //! Ergonomic re-exports for the secp256k1 scheme.

    pub use crate::schemes::secp256k1::*;
    #[cfg(feature = "std")]
    pub type Signer = crate::Signer<Secp256k1>;
    pub type MemoryStorage = crate::storage::MemoryKeyStorage<Secp256k1>;
    #[cfg(feature = "std")]
    pub type FileStorage = crate::storage::FSKeyStorage<Secp256k1>;
}

#[cfg(feature = "sr25519")]
pub mod sr25519 {
    //! Ergonomic re-exports for the sr25519 scheme.

    #[cfg(feature = "sp-core")]
    pub use crate::substrate::sp_compat;
    pub use crate::{schemes::sr25519::*, substrate::SubstratePair};
    pub type Signer = crate::Signer<Sr25519>;
    pub type MemoryStorage = crate::storage::MemoryKeyStorage<Sr25519>;
    pub type FileStorage = crate::storage::FSKeyStorage<Sr25519>;
}

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{KeyStorage, SignatureScheme, schemes};

    #[cfg(feature = "secp256k1")]
    pub use crate::Address;

    #[cfg(any(feature = "secp256k1", feature = "sr25519"))]
    pub use crate::schemes::SchemeType;

    #[cfg(feature = "std")]
    pub use crate::Signer;

    #[cfg(feature = "secp256k1")]
    pub use crate::schemes::secp256k1::Secp256k1;

    #[cfg(feature = "sr25519")]
    pub use crate::schemes::sr25519::Sr25519;
}
