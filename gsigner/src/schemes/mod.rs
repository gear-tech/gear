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

//! Cryptographic signature scheme implementations.

use crate::traits::SignatureScheme;

#[cfg(feature = "secp256k1")]
pub mod secp256k1;

#[cfg(feature = "sr25519")]
pub mod sr25519;

#[cfg(feature = "ed25519")]
pub mod ed25519;

/// Enumeration of supported signature schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemeType {
    #[cfg(feature = "secp256k1")]
    /// secp256k1 ECDSA (Ethereum).
    Secp256k1,

    #[cfg(feature = "sr25519")]
    /// sr25519 Schnorrkel (Substrate).
    Sr25519,

    #[cfg(feature = "ed25519")]
    /// ed25519 Edwards-curve digital signatures.
    Ed25519,
}

impl SchemeType {
    /// Get the name of the scheme.
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "secp256k1")]
            SchemeType::Secp256k1 => crate::schemes::secp256k1::Secp256k1::scheme_name(),
            #[cfg(feature = "sr25519")]
            SchemeType::Sr25519 => crate::schemes::sr25519::Sr25519::scheme_name(),
            #[cfg(feature = "ed25519")]
            SchemeType::Ed25519 => crate::schemes::ed25519::Ed25519::scheme_name(),
            #[allow(unreachable_patterns)]
            _ => unreachable!("No signature schemes enabled"),
        }
    }
}
