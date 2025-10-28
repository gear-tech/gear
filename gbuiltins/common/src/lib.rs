// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Gear Builtin Actors Common Types
//!
//! This crate provides common types shared across all Gear builtin actors,
//! including identification and type enumeration.
//!
//! ## Builtin Actor Identification
//!
//! Builtin actors are identified using a pallet-style naming convention with version support:
//!
//! ```rust
//! use gbuiltin_common::BuiltinActorId;
//!
//! // Create a builtin actor ID
//! let builtin_id = BuiltinActorId::new(b"staking", 1);
//! assert_eq!(builtin_id.version, 1);
//! ```
//!
//! The encoding follows the pattern: `modl/bia/{name}/v-{version}/`
//! where:
//! - `modl` = module (Substrate convention)
//! - `bia` = builtin actor
//! - `{name}` = actor name (max 16 bytes)
//! - `v-{version}` = version number (u16, little-endian)
//!
//! ## Available Builtin Actor Types
//!
//! - **Staking** (`b"staking"`, v1): Substrate staking operations
//! - **Proxy** (`b"proxy"`, v1): Proxy account management
//! - **BLS12-381** (`b"bls12-381"`, v1): BLS12-381 cryptographic operations
//! - **Eth Bridge** (`b"eth-bridge"`, v1): Ethereum bridge operations

#![no_std]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use parity_scale_codec::{Decode, Encode, Error, Input, Output};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use core::fmt;

/// Builtin Actor ID with name and version.
///
/// This structure uniquely identifies a builtin actor by its name and version number.
/// The encoding follows Substrate's pallet module ID conventions.
///
/// # Examples
///
/// ```
/// # use gbuiltin_common::BuiltinActorId;
/// let id = BuiltinActorId::new(b"staking", 1);
/// assert_eq!(id.version, 1);
/// ```
#[derive(Clone, Copy, Default, Eq, PartialEq, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize, Debug))]
pub struct BuiltinActorId {
    /// The unique name of the builtin actor (max 16 bytes).
    pub name: [u8; 16],
    /// The version of the builtin actor.
    pub version: u16,
}

impl BuiltinActorId {
    /// Creates a new `BuiltinActorId` with the given name and version.
    ///
    /// # Panics
    ///
    /// Panics if the name is empty or longer than 16 bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gbuiltin_common::BuiltinActorId;
    /// let id = BuiltinActorId::new(b"staking", 1);
    /// assert_eq!(id.version, 1);
    /// ```
    pub const fn new(name: &[u8], version: u16) -> Self {
        assert!(!name.is_empty(), "Actor name cannot be empty");
        assert!(name.len() <= 16, "Actor name too long (max 16 bytes)");

        let mut name_arr = [0u8; 16];
        let mut i = 0;

        // Copy the name into the array.
        while i < name.len() {
            name_arr[i] = name[i];
            i += 1;
        }

        Self {
            name: name_arr,
            version,
        }
    }

    /// Returns the length of the actor name (excluding padding zeros).
    pub const fn name_len(&self) -> usize {
        let mut len = 0;
        while len < 16 && self.name[len] != 0 {
            len += 1;
        }
        len
    }
}

#[cfg(feature = "std")]
impl fmt::Display for BuiltinActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name_len = self.name_len();
        let name_str = core::str::from_utf8(&self.name[..name_len]).unwrap_or("<invalid-utf8>");
        write!(f, "{}/v{}", name_str, self.version)
    }
}

impl Encode for BuiltinActorId {
    fn size_hint(&self) -> usize {
        // Always encoded as 32 bytes (ActorId size)
        32
    }

    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let name_len = self.name_len();

        // Build a 32-byte buffer with the encoded data, padded with zeros
        let mut buf = [0u8; 32];
        let mut pos = 0;

        // Helper macro to write slice and advance position
        macro_rules! write_slice {
            ($slice:expr) => {{
                let slice = $slice;
                let len = slice.len();
                buf[pos..pos + len].copy_from_slice(slice);
                pos += len;
            }};
        }

        write_slice!(b"modl/bia/");
        write_slice!(&self.name[..name_len]);
        write_slice!(b"/v-");
        write_slice!(&self.version.to_le_bytes());
        buf[pos] = b'/';

        // Remaining bytes are already zero-padded
        dest.write(&buf);
    }
}

impl Decode for BuiltinActorId {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        // BuiltinActorId is always encoded as exactly 32 bytes (ActorId size)
        // Shorter encodings are padded with zeros
        let mut bytes = [0u8; 32];
        input.read(&mut bytes)?;

        let mut parts = bytes.split(|&x| x == b'/');

        // Check for "modl" prefix
        if parts.next() != Some(b"modl") {
            return Err("BuiltinActorId decode: expected prefix 'modl'".into());
        }

        // Check for "bia" prefix
        if parts.next() != Some(b"bia") {
            return Err("BuiltinActorId decode: expected prefix 'modl/bia'".into());
        }

        // Extract actor name
        let name_bytes = parts
            .next()
            .ok_or("BuiltinActorId decode: missing actor name after 'modl/bia/' prefix")?;

        if name_bytes.len() > 16 {
            return Err("BuiltinActorId decode: actor name too long (max 16 bytes)".into());
        }

        if name_bytes.is_empty() {
            return Err("BuiltinActorId decode: actor name is empty".into());
        }

        let mut name = [0u8; 16];
        name[..name_bytes.len()].copy_from_slice(name_bytes);

        // Extract version
        let version_bytes = parts
            .next()
            .ok_or("BuiltinActorId decode: missing version field")?;

        if !version_bytes.starts_with(b"v-") {
            return Err("BuiltinActorId decode: version must start with 'v-' prefix".into());
        }

        // Skip "v-" prefix and get version bytes
        if version_bytes.len() < 4 {
            return Err("BuiltinActorId decode: incomplete version data".into());
        }

        let version_number = &version_bytes[2..4];
        let mut version_arr = [0u8; 2];
        version_arr.copy_from_slice(version_number);
        let version = u16::from_le_bytes(version_arr);

        Ok(BuiltinActorId { name, version })
    }
}

/// Built-in actor types enumeration.
///
/// This enum represents all available builtin actor types in the runtime.
/// Each type has an associated `BuiltinActorId` that defines its name and version.
#[derive(Copy, Clone, Default, Eq, PartialEq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize, Debug))]
pub enum BuiltinActorType {
    /// Custom builtin actor (for testing purposes only)
    ///
    /// Only available with the `dev` feature flag.
    /// **Warning:** This variant should only be used in tests.
    #[cfg(any(test, feature = "dev"))]
    Custom(BuiltinActorId),
    /// Default case for unknown actors
    #[default]
    Unknown,
    /// Staking builtin actor
    Staking,
    /// Proxy builtin actor
    Proxy,
    /// BLS12-381 cryptographic operations actor
    BLS12_381,
    /// Ethereum bridge actor
    EthBridge,
}

impl BuiltinActorType {
    /// Returns the `BuiltinActorId` for the given actor type.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gbuiltin_common::{BuiltinActorType, BuiltinActorId};
    /// let staking_type = BuiltinActorType::Staking;
    /// let id = staking_type.id();
    /// assert_eq!(id, BuiltinActorId::new(b"staking", 1));
    /// ```
    pub const fn id(&self) -> BuiltinActorId {
        match self {
            #[cfg(any(test, feature = "dev"))]
            Self::Custom(id) => *id,
            Self::Unknown => BuiltinActorId::new(b"unknown", 0),
            Self::Staking => BuiltinActorId::new(b"staking", 1),
            Self::Proxy => BuiltinActorId::new(b"proxy", 1),
            Self::BLS12_381 => BuiltinActorId::new(b"bls12-381", 1),
            Self::EthBridge => BuiltinActorId::new(b"eth-bridge", 1),
        }
    }

    /// Returns the `BuiltinActorType` for a legacy numeric ID.
    ///
    /// This method provides backward compatibility with the old numeric ID system.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gbuiltin_common::BuiltinActorType;
    /// assert_eq!(
    ///     BuiltinActorType::from_index(1),
    ///     Some(BuiltinActorType::BLS12_381)
    /// );
    /// assert_eq!(
    ///     BuiltinActorType::from_index(2),
    ///     Some(BuiltinActorType::Staking)
    /// );
    /// assert_eq!(BuiltinActorType::from_index(999), None);
    /// ```
    pub const fn from_index(index: u64) -> Option<Self> {
        match index {
            1 => Some(BuiltinActorType::BLS12_381),
            2 => Some(BuiltinActorType::Staking),
            3 => Some(BuiltinActorType::EthBridge),
            4 => Some(BuiltinActorType::Proxy),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn builtin_actor_id_new_works() {
        let id = BuiltinActorId::new(b"staking", 1);
        assert_eq!(id.version, 1);
        assert_eq!(id.name_len(), 7);
    }

    #[test]
    #[should_panic(expected = "Actor name cannot be empty")]
    fn builtin_actor_id_empty_name_panics() {
        let _ = BuiltinActorId::new(b"", 1);
    }

    #[test]
    #[should_panic(expected = "Actor name too long")]
    fn builtin_actor_id_long_name_panics() {
        let _ = BuiltinActorId::new(b"this-is-way-too-long", 1);
    }

    #[test]
    fn builtin_actor_id_encode_decode_roundtrip() {
        let original = BuiltinActorId::new(b"staking", 1);
        let encoded = original.encode();

        // Verify encoding is always exactly 32 bytes
        assert_eq!(
            encoded.len(),
            32,
            "BuiltinActorId must always encode to 32 bytes"
        );

        let decoded = BuiltinActorId::decode(&mut &encoded[..]).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn builtin_actor_id_encoding_is_always_32_bytes() {
        // Test various name lengths
        let short = BuiltinActorId::new(b"abc", 1);
        assert_eq!(short.encode().len(), 32);

        let medium = BuiltinActorId::new(b"staking", 1);
        assert_eq!(medium.encode().len(), 32);

        let long = BuiltinActorId::new(b"bls12-381", 1);
        assert_eq!(long.encode().len(), 32);

        let max_len = BuiltinActorId::new(b"sixteenbytesname", 1);
        assert_eq!(max_len.encode().len(), 32);
    }

    #[test]
    fn builtin_actor_type_id_mapping() {
        assert_eq!(
            BuiltinActorType::Staking.id(),
            BuiltinActorId::new(b"staking", 1)
        );
        assert_eq!(
            BuiltinActorType::Proxy.id(),
            BuiltinActorId::new(b"proxy", 1)
        );
    }

    #[test]
    fn builtin_actor_type_from_index_works() {
        assert_eq!(
            BuiltinActorType::from_index(1),
            Some(BuiltinActorType::BLS12_381)
        );
        assert_eq!(
            BuiltinActorType::from_index(2),
            Some(BuiltinActorType::Staking)
        );
        assert_eq!(
            BuiltinActorType::from_index(3),
            Some(BuiltinActorType::EthBridge)
        );
        assert_eq!(
            BuiltinActorType::from_index(4),
            Some(BuiltinActorType::Proxy)
        );
        assert_eq!(BuiltinActorType::from_index(999), None);
    }

    #[cfg(feature = "std")]
    #[test]
    fn builtin_actor_id_display_works() {
        let id = BuiltinActorId::new(b"staking", 1);
        assert_eq!(format!("{}", id), "staking/v1");
    }
}
