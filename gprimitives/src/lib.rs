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

//! Gear primitive types.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

extern crate alloc;

pub use gear_ss58::Ss58Address;
pub use nonzero_u256::NonZeroU256;
pub use primitive_types::{H160, H256, U256};

pub mod utils;

mod macros;
mod nonzero_u256;
#[cfg(feature = "ethexe")]
mod sol_types;

use core::{
    fmt,
    str::{self, FromStr},
};
use derive_more::{AsMut, AsRef, From, Into};
use gear_ss58::RawSs58Address;
#[cfg(feature = "codec")]
use scale_decode::DecodeAsType;
#[cfg(feature = "codec")]
use scale_encode::EncodeAsType;
#[cfg(feature = "codec")]
use scale_info::{
    TypeInfo,
    scale::{self, Decode, Encode, MaxEncodedLen},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// The error type returned when conversion fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConversionError {
    /// Invalid slice length.
    #[error("Slice should be 32 length")]
    InvalidSliceLength,
    /// Invalid hex string.
    #[error("Invalid hex string")]
    InvalidHexString,
    /// Invalid SS58 address.
    #[error("Invalid SS58 address")]
    InvalidSs58Address,
    /// SS58 encoding failed.
    #[error("SS58 encoding failed")]
    Ss58Encode,
}

/// Message handle.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Message creation consists of the following parts: message
/// initialization, filling the message with payload (can be gradual), and
/// message sending.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, From, Into)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, EncodeAsType, Decode, DecodeAsType, MaxEncodedLen), codec(crate = scale))]
pub struct MessageHandle(u32);

/// Program (actor) identifier.
///
/// Gear allows user and program interactions via messages. Source and target
/// program as well as user are represented by 256-bit identifier `ActorId`
/// struct. The source `ActorId` for a message being processed can be obtained
/// using `gstd::msg::source()` function. Also, each send function has a target
/// `ActorId` as one of the arguments.
///
/// NOTE: Implementation of `From<u64>` places bytes from idx=12 for Eth compatibility.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, EncodeAsType, Decode, DecodeAsType, MaxEncodedLen), codec(crate = scale))]
pub struct ActorId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_h256 into_h256 try_from_slice debug, ActorId);

impl ActorId {
    /// Returns the ss58-check address with default ss58 version.
    pub fn to_ss58check(&self) -> Result<Ss58Address, ConversionError> {
        RawSs58Address::from(self.0)
            .to_ss58check()
            .map_err(|_| ConversionError::Ss58Encode)
    }

    /// Returns the ss58-check address with given ss58 version.
    pub fn to_ss58check_with_version(&self, version: u16) -> Result<Ss58Address, ConversionError> {
        RawSs58Address::from(self.0)
            .to_ss58check_with_prefix(version)
            .map_err(|_| ConversionError::Ss58Encode)
    }

    /// Returns [`H160`] with possible loss of the first 12 bytes.
    pub fn to_address_lossy(&self) -> H160 {
        let mut h160 = H160::zero();
        h160.0.copy_from_slice(&self.into_bytes()[12..]);
        h160
    }
}

impl From<u64> for ActorId {
    fn from(value: u64) -> Self {
        let mut id = Self::zero();
        id.0[12..20].copy_from_slice(&value.to_le_bytes()[..]);
        id
    }
}

impl From<H160> for ActorId {
    fn from(h160: H160) -> Self {
        let mut actor_id = Self::zero();
        actor_id.0[12..].copy_from_slice(h160.as_ref());
        actor_id
    }
}

impl TryInto<H160> for ActorId {
    type Error = &'static str;

    fn try_into(self) -> Result<H160, Self::Error> {
        if !self.0[..12].iter().all(|i| i.eq(&0)) {
            Err("ActorId has non-zero prefix")
        } else {
            let mut h160 = H160::zero();
            h160.0.copy_from_slice(&self.into_bytes()[12..]);
            Ok(h160)
        }
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let byte_array = utils::ByteSliceFormatter::Array(&self.0);

        let is_alternate = f.alternate();
        if is_alternate {
            f.write_str(concat!(stringify!(ActorId), "("))?;
        }

        let sign_plus = f.sign_plus();
        let width = f.width();

        if sign_plus && width.is_some() {
            return Err(fmt::Error);
        }

        let version = if sign_plus {
            Some(gear_ss58::VARA_SS58_PREFIX)
        } else if let Some(version) = width {
            Some(version.try_into().map_err(|_| fmt::Error)?)
        } else {
            None
        };

        if let Some(version) = version {
            let address = self
                .to_ss58check_with_version(version)
                .map_err(|_| fmt::Error)?;
            let address_str = address.as_str();

            let len = address.as_str().len();
            let median = len.div_ceil(2);

            let mut e1 = median;
            let mut s2 = median;

            if let Some(precision) = f.precision()
                && precision < median
            {
                e1 = precision;
                s2 = len - precision;
            }

            let p1 = &address_str[..e1];
            let p2 = &address_str[s2..];
            let sep = if e1.ne(&s2) { ".." } else { Default::default() };

            write!(f, "{p1}{sep}{p2}")?;
        } else {
            byte_array.fmt(f)?;
        }

        if is_alternate {
            f.write_str(")")?;
        }

        Ok(())
    }
}

impl FromStr for ActorId {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let actod_id = if let Some(s) = s.strip_prefix("0x") {
            if s.len() != 64 {
                return Err(ConversionError::InvalidHexString);
            }
            let mut actor_id = Self::zero();
            hex::decode_to_slice(s, &mut actor_id.0)
                .map_err(|_| ConversionError::InvalidHexString)?;
            actor_id
        } else {
            let raw_address = RawSs58Address::from_ss58check(s)
                .map_err(|_| ConversionError::InvalidSs58Address)?
                .into();
            Self::new(raw_address)
        };

        Ok(actod_id)
    }
}

#[cfg(all(feature = "serde", not(feature = "ethexe")))]
impl Serialize for ActorId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let address = self
            .to_ss58check_with_version(gear_ss58::VARA_SS58_PREFIX)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(address.as_str())
    }
}

#[cfg(all(feature = "serde", feature = "ethexe"))]
impl Serialize for ActorId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let id: H160 = self.to_address_lossy();
        id.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for ActorId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ActorIdVisitor;

        impl de::Visitor<'_> for ActorIdVisitor {
            type Value = ActorId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in SS58 format")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let raw_address = RawSs58Address::from_ss58check(value)
                    .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(value), &self))?
                    .into();
                Ok(Self::Value::new(raw_address))
            }
        }

        deserializer.deserialize_identifier(ActorIdVisitor)
    }
}

/// Message identifier.
///
/// Gear allows users and program interactions via messages. Each message has
/// its own unique 256-bit id. This id is represented via the `MessageId`
/// struct. The message identifier can be obtained for the currently processed
/// message using the `gstd::msg::id()` function. Also, each send and reply
/// functions return a message identifier.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, EncodeAsType, Decode, DecodeAsType, MaxEncodedLen), codec(crate = scale))]
pub struct MessageId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 into_h256 from_str display debug serde, MessageId);

/// Code identifier.
///
/// This identifier can be obtained as a result of executing the
/// `gear.uploadCode` extrinsic. Actually, the code identifier is the Blake2
/// hash of the Wasm binary code blob.
///
/// Code identifier is required when creating programs from programs (see
/// `gstd::prog` module for details).
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, EncodeAsType, Decode, DecodeAsType, MaxEncodedLen), codec(crate = scale))]
pub struct CodeId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 into_h256 from_str try_from_slice display debug serde, CodeId);

/// Reservation identifier.
///
/// The identifier is used to reserve and unreserve gas amount for program
/// execution later.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, EncodeAsType, Decode, DecodeAsType, MaxEncodedLen), codec(crate = scale))]
pub struct ReservationId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 into_h256 from_str display debug serde, ReservationId);

#[cfg(test)]
mod tests {
    extern crate alloc;

    use crate::{ActorId, H160};
    use alloc::format;
    use core::str::FromStr;

    fn actor_id() -> ActorId {
        ActorId::from_str("0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e")
            .unwrap()
    }

    /// Test that ActorId cannot be formatted using
    /// Vara format and custom version at the same time.
    #[test]
    #[should_panic]
    fn duplicate_version_in_actor_id_fmt_test() {
        let id = actor_id();
        let _ = format!("{id:+42}");
    }

    #[test]
    fn formatting_test() {
        let id = actor_id();

        // `Debug`/`Display`.
        assert_eq!(
            format!("{id:?}"),
            "0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e"
        );
        // `Debug`/`Display` with precision 0.
        assert_eq!(format!("{id:.0?}"), "0x..");
        // `Debug`/`Display` with precision 1.
        assert_eq!(format!("{id:.1?}"), "0x6a..3e");
        // `Debug`/`Display` with precision 2.
        assert_eq!(format!("{id:.2?}"), "0x6a51..ed3e");
        // `Debug`/`Display` with precision 4.
        assert_eq!(format!("{id:.4?}"), "0x6a519a19..f765ed3e");
        // `Debug`/`Display` with precision 15.
        assert_eq!(
            format!("{id:.15?}"),
            "0x6a519a19ffdfd8f45c310b44aecf15..0c713bf841a8cb695b0ea5f765ed3e"
        );
        // `Debug`/`Display` with precision 30 (the same for any case >= 16).
        assert_eq!(
            format!("{id:.30?}"),
            "0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e"
        );
        // `Debug`/`Display` with sign + (vara address).
        assert_eq!(
            format!("{id:+}"),
            "kGhwPiWGsCZkaUNqotftspabNLRTcNoMe5APCSDJM2uJv6PSm"
        );
        // `Debug`/`Display` with width (custom address, 42 means substrate).
        assert_eq!(
            format!("{id:42}"),
            "5EU7B2s4m2XrgSbUyt8U92fDpSi2EtW3Z3kKwUW4drZ1KAZD"
        );
        // `Debug`/`Display` with sign + (vara address) and with precision 0.
        assert_eq!(format!("{id:+.0}"), "..");
        // `Debug`/`Display` with sign + (vara address) and with precision 1.
        assert_eq!(format!("{id:+.1}"), "k..m");
        // `Debug`/`Display` with sign + (vara address) and with precision 2.
        assert_eq!(format!("{id:+.2}"), "kG..Sm");
        // `Debug`/`Display` with sign + (vara address) and with precision 4.
        assert_eq!(format!("{id:+.4}"), "kGhw..6PSm");
        // `Debug`/`Display` with sign + (vara address) and with precision 15.
        assert_eq!(format!("{id:+.15}"), "kGhwPiWGsCZkaUN..APCSDJM2uJv6PSm");
        // `Debug`/`Display` with sign + (vara address) and with precision 25 (the same for any case >= 25).
        assert_eq!(
            format!("{id:+.25}"),
            "kGhwPiWGsCZkaUNqotftspabNLRTcNoMe5APCSDJM2uJv6PSm"
        );
        // Alternate formatter.
        assert_eq!(
            format!("{id:#}"),
            "ActorId(0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e)"
        );
        // Alternate formatter with precision 2.
        assert_eq!(format!("{id:#.2}"), "ActorId(0x6a51..ed3e)");
        // Alternate formatter with precision 2.
        assert_eq!(format!("{id:+#.2}"), "ActorId(kG..Sm)");
        // Alternate formatter with sign + (vara address).
        assert_eq!(
            format!("{id:+#}"),
            "ActorId(kGhwPiWGsCZkaUNqotftspabNLRTcNoMe5APCSDJM2uJv6PSm)"
        );
        // Alternate formatter with width (custom address, 42 means substrate).
        assert_eq!(
            format!("{id:#42}"),
            "ActorId(5EU7B2s4m2XrgSbUyt8U92fDpSi2EtW3Z3kKwUW4drZ1KAZD)"
        );
    }

    /// Test that ActorId's `try_from(bytes)` constructor causes panic
    /// when the argument has the wrong length
    #[test]
    fn actor_id_from_slice_error_implementation() {
        let bytes = "foobar";
        let result: Result<ActorId, _> = bytes.as_bytes().try_into();
        assert!(result.is_err());
    }

    #[test]
    fn actor_id_ethereum_address() {
        let address: H160 = "0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5"
            .parse()
            .unwrap();
        assert_eq!(
            format!("{address:?}"),
            "0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5"
        );

        let actor_id: ActorId = address.into();
        assert_eq!(
            format!("{actor_id}"),
            "0x00000000000000000000000095222290dd7278aa3ddd389cc1e1d165cc4bafe5"
        );

        let address = actor_id.to_address_lossy();
        assert_eq!(
            format!("{address:?}"),
            "0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5"
        );
    }
}
