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

//! PeerId helpers for supported key schemes.
//!
//! Notes:
//! - libp2p supports PeerId generation for secp256k1 and ed25519.
//! - sr25519 has no canonical PeerId in libp2p; we skip it.

use crate::error::{Result, SignerError};

/// Compute PeerId for a secp256k1 public key.
#[cfg(feature = "secp256k1")]
pub fn peer_id_from_secp256k1(
    public: &crate::schemes::secp256k1::PublicKey,
) -> Result<libp2p_identity::PeerId> {
    let key = libp2p_identity::secp256k1::PublicKey::try_from_bytes(&public.to_bytes())
        .map_err(|e| SignerError::InvalidKey(format!("Invalid secp256k1 for PeerId: {e}")))?;
    Ok(libp2p_identity::PeerId::from_public_key(
        &libp2p_identity::PublicKey::from(key),
    ))
}

/// Compute PeerId for an ed25519 public key.
#[cfg(feature = "ed25519")]
pub fn peer_id_from_ed25519(
    public: &crate::schemes::ed25519::PublicKey,
) -> Result<libp2p_identity::PeerId> {
    let key = libp2p_identity::ed25519::PublicKey::try_from_bytes(&public.to_bytes())
        .map_err(|e| SignerError::InvalidKey(format!("Invalid ed25519 for PeerId: {e}")))?;
    Ok(libp2p_identity::PeerId::from_public_key(
        &libp2p_identity::PublicKey::from(key),
    ))
}
