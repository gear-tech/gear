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

//! PeerId helpers for supported key schemes.
//!
//! This module provides a [`ToPeerId`] trait for deriving libp2p PeerIds from
//! public keys, as well as standalone helper functions.
//!
//! # Supported Schemes
//! - **secp256k1**: Full support via libp2p's secp256k1 identity
//! - **ed25519**: Full support via libp2p's ed25519 identity
//! - **sr25519**: Not supported (no canonical PeerId representation in libp2p)
//!
//! # Example
//! ```ignore
//! use gsigner::peer_id::ToPeerId;
//! use gsigner::secp256k1::Signer;
//!
//! let signer = Signer::memory();
//! let public = signer.generate()?;
//! let peer_id = public.to_peer_id()?;
//! ```

use crate::error::{Result, SignerError};
pub use libp2p_identity::PeerId;

/// Trait for types that can derive a libp2p PeerId.
///
/// This trait is implemented for public key types from schemes that have
/// canonical PeerId representations in libp2p (secp256k1 and ed25519).
pub trait ToPeerId {
    /// Derive a libp2p PeerId from this public key.
    fn to_peer_id(&self) -> Result<PeerId>;
}

#[cfg(feature = "secp256k1")]
impl ToPeerId for crate::schemes::secp256k1::PublicKey {
    fn to_peer_id(&self) -> Result<PeerId> {
        peer_id_from_secp256k1(self)
    }
}

/// Compute PeerId for a secp256k1 public key.
#[cfg(feature = "secp256k1")]
pub fn peer_id_from_secp256k1(public: &crate::schemes::secp256k1::PublicKey) -> Result<PeerId> {
    let key = libp2p_identity::secp256k1::PublicKey::try_from_bytes(&public.to_bytes())
        .map_err(|e| SignerError::InvalidKey(format!("Invalid secp256k1 for PeerId: {e}")))?;
    Ok(PeerId::from_public_key(&libp2p_identity::PublicKey::from(
        key,
    )))
}

#[cfg(feature = "ed25519")]
impl ToPeerId for crate::schemes::ed25519::PublicKey {
    fn to_peer_id(&self) -> Result<PeerId> {
        peer_id_from_ed25519(self)
    }
}

/// Compute PeerId for an ed25519 public key.
#[cfg(feature = "ed25519")]
pub fn peer_id_from_ed25519(public: &crate::schemes::ed25519::PublicKey) -> Result<PeerId> {
    let key = libp2p_identity::ed25519::PublicKey::try_from_bytes(&public.to_bytes())
        .map_err(|e| SignerError::InvalidKey(format!("Invalid ed25519 for PeerId: {e}")))?;
    Ok(PeerId::from_public_key(&libp2p_identity::PublicKey::from(
        key,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "secp256k1")]
    #[test]
    fn test_secp256k1_to_peer_id() {
        use crate::schemes::secp256k1::PrivateKey;

        let private = PrivateKey::random();
        let public = private.public_key();

        // Both trait method and function should produce the same result
        let peer_id_trait = public.to_peer_id().unwrap();
        let peer_id_fn = peer_id_from_secp256k1(&public).unwrap();

        assert_eq!(peer_id_trait, peer_id_fn);
        assert!(!peer_id_trait.to_string().is_empty());
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn test_ed25519_to_peer_id() {
        use crate::schemes::ed25519::PrivateKey;

        let private = PrivateKey::random();
        let public = private.public_key();

        // Both trait method and function should produce the same result
        let peer_id_trait = public.to_peer_id().unwrap();
        let peer_id_fn = peer_id_from_ed25519(&public).unwrap();

        assert_eq!(peer_id_trait, peer_id_fn);
        assert!(!peer_id_trait.to_string().is_empty());
    }

    #[cfg(all(feature = "secp256k1", feature = "ed25519"))]
    #[test]
    fn test_different_schemes_produce_different_peer_ids() {
        use crate::schemes::{
            ed25519::PrivateKey as Ed25519Private, secp256k1::PrivateKey as Secp256k1Private,
        };

        let secp_private = Secp256k1Private::random();
        let secp_public = secp_private.public_key();
        let secp_peer_id = secp_public.to_peer_id().unwrap();

        let ed_private = Ed25519Private::random();
        let ed_public = ed_private.public_key();
        let ed_peer_id = ed_public.to_peer_id().unwrap();

        // Different schemes should produce different peer IDs
        assert_ne!(secp_peer_id, ed_peer_id);
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn test_deterministic_peer_id() {
        use crate::schemes::secp256k1::PrivateKey;

        let private = PrivateKey::random();
        let public = private.public_key();

        // Same public key should always produce the same peer ID
        let peer_id_1 = public.to_peer_id().unwrap();
        let peer_id_2 = public.to_peer_id().unwrap();

        assert_eq!(peer_id_1, peer_id_2);
    }
}
