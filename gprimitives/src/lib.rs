// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

pub use gear_ss58::Ss58Address;

pub mod macros;
mod utils;

use core::{
    fmt,
    str::{self, FromStr},
};
use derive_more::{AsMut, AsRef, Display, From, Into};
use gear_ss58::RawSs58Address;
#[cfg(feature = "codec")]
use {
    primitive_types::H256,
    scale_info::{
        scale::{self, Decode, Encode, MaxEncodedLen},
        TypeInfo,
    },
};

/// The error type returned when conversion fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum ConversionError {
    /// Invalid slice length.
    #[display(fmt = "Slice should be 32 length")]
    InvalidSliceLength,
    /// Invalid hex string.
    #[display(fmt = "Invalid hex string")]
    InvalidHexString,
    /// Invalid SS58 address.
    #[display(fmt = "Invalid SS58 address")]
    InvalidSs58Address,
    /// SS58 encoding failed.
    #[display(fmt = "SS58 encoding failed")]
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
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct MessageHandle(u32);

/// Program (actor) identifier.
///
/// Gear allows user and program interactions via messages. Source and target
/// program as well as user are represented by 256-bit identifier `ActorId`
/// struct. The source `ActorId` for a message being processed can be obtained
/// using `msg::source()` function. Also, each send function has a target
/// `ActorId` as one of the arguments.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct ActorId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice debug, ActorId);

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
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let byte_array = utils::ByteArray(&self.0);

        let is_alternate = f.alternate();
        if is_alternate {
            f.write_str(concat!(stringify!(ActorId), "("))?;
        }

        let version = if f.sign_plus() {
            Some(gear_ss58::VARA_SS58_PREFIX)
        } else if let Some(version) = f.width() {
            Some(version.try_into().map_err(|_| fmt::Error)?)
        } else {
            None
        };

        if let Some(version) = version {
            self.to_ss58check_with_version(version)
                .map_err(|_| fmt::Error)?
                .fmt(f)?;
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
        let mut actor_id = Self::zero();

        if let Some(s) = s.strip_prefix("0x") {
            if s.len() != 64 {
                return Err(ConversionError::InvalidHexString);
            }
            hex::decode_to_slice(s, &mut actor_id.0)
                .map_err(|_| ConversionError::InvalidHexString)?;
        } else {
            let raw_address: [u8; 32] = RawSs58Address::from_ss58check(s)
                .map(Into::into)
                .map_err(|_| ConversionError::InvalidSs58Address)?;
            actor_id.0[..].copy_from_slice(&raw_address);
        }

        Ok(actor_id)
    }
}

/// Message identifier.
///
/// Gear allows users and program interactions via messages. Each message has
/// its own unique 256-bit id. This id is represented via the `MessageId`
/// struct. The message identifier can be obtained for the currently processed
/// message using the `msg::id()` function. Also, each send and reply functions
/// return a message identifier.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct MessageId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 display debug, MessageId);

/// Code identifier.
///
/// This identifier can be obtained as a result of executing the
/// `gear.uploadCode` extrinsic. Actually, the code identifier is the Blake2
/// hash of the Wasm binary code blob.
///
/// Code identifier is required when creating programs from programs (see `prog`
/// module for details).
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct CodeId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice display debug, CodeId);

impl FromStr for CodeId {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.strip_prefix("0x") {
            Some(s) if s.len() == 64 => {
                let mut code_id = Self::zero();
                hex::decode_to_slice(s, &mut code_id.0)
                    .map_err(|_| ConversionError::InvalidHexString)?;
                Ok(code_id)
            }
            _ => Err(ConversionError::InvalidHexString),
        }
    }
}

/// Reservation identifier.
///
/// The identifier is used to reserve and unreserve gas amount for program
/// execution later.
#[derive(Clone, Copy, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct ReservationId([u8; 32]);

macros::impl_primitive!(new zero into_bytes from_u64 from_h256 display debug, ReservationId);
