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
//! supporting secp256k1 (Ethereum), ed25519, and sr25519 (Substrate) signature schemes.
//!
//! # Features
//!
//! - `secp256k1` - Enable Ethereum/secp256k1 ECDSA support (enabled by default)
//! - `sr25519` - Enable Substrate/sr25519 Schnorrkel support (enabled by default)
//! - `ed25519` - Enable Substrate-compatible ed25519 support (enabled by default)
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod address;
pub mod crypto;
pub mod error;
#[cfg(feature = "secp256k1")]
pub mod hash;
#[cfg(feature = "peer-id")]
pub mod peer_id;
pub mod schemes;
#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub mod signer;
pub mod traits;
pub mod utils;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "keyring")]
pub mod keyring;

#[cfg(any(feature = "sr25519", feature = "ed25519", feature = "secp256k1"))]
pub mod substrate;
#[cfg(any(feature = "sr25519", feature = "ed25519", feature = "secp256k1"))]
pub use substrate as substrate_utils;

#[cfg(feature = "secp256k1")]
pub use address::Address;
#[cfg(feature = "secp256k1")]
pub use address::FromActorIdError;
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
pub use address::{SubstrateAddress, SubstrateCryptoScheme};
pub use error::{Result, SignerError};
#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub use signer::Signer;
#[cfg(feature = "ed25519")]
pub use substrate::Ed25519Pair;
#[cfg(feature = "secp256k1")]
pub use substrate::Secp256k1Pair;
#[cfg(feature = "sr25519")]
pub use substrate::{Sr25519Pair, SubstratePair};
pub use traits::SignatureScheme;

#[cfg(feature = "secp256k1")]
pub use schemes::secp256k1::{
    ContractSignature, Digest, PrivateKey, PublicKey, Signature, SignedData, SignedMessage,
    ToDigest, VerifiedData,
};

#[cfg(feature = "ed25519")]
pub use schemes::ed25519::Ed25519;

#[cfg(feature = "secp256k1")]
pub mod secp256k1 {
    //! Ergonomic re-exports for the secp256k1 scheme.

    pub use crate::{schemes::secp256k1::*, substrate::Secp256k1Pair};
    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub type Signer = crate::Signer<Secp256k1>;
}

#[cfg(feature = "sr25519")]
pub mod sr25519 {
    //! Ergonomic re-exports for the sr25519 scheme.

    pub use crate::{
        schemes::sr25519::*,
        substrate::{Sr25519Pair, SubstratePair, sp_compat},
    };
    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub type Signer = crate::Signer<Sr25519>;
}

#[cfg(feature = "ed25519")]
pub mod ed25519 {
    //! Ergonomic re-exports for the ed25519 scheme.

    pub use crate::{schemes::ed25519::*, substrate::Ed25519Pair};
    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub type Signer = crate::Signer<Ed25519>;
}

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{SignatureScheme, schemes};

    #[cfg(feature = "secp256k1")]
    pub use crate::Address;

    #[cfg(any(feature = "secp256k1", feature = "sr25519", feature = "ed25519"))]
    pub use crate::schemes::SchemeType;

    #[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
    pub use crate::Signer;

    #[cfg(feature = "secp256k1")]
    pub use crate::schemes::secp256k1::Secp256k1;

    #[cfg(feature = "sr25519")]
    pub use crate::schemes::sr25519::Sr25519;

    #[cfg(feature = "ed25519")]
    pub use crate::schemes::ed25519::Ed25519;
}
