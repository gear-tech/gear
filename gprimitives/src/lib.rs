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

pub use gear_ss58::{RawSs58Address, Ss58Address};

pub mod utils;

use core::{
    fmt,
    str::{self, FromStr},
};
use derive_more::{AsMut, AsRef, Display, From, Into};
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

/// Program identifier.
pub type ProgramId = ActorId;

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

utils::impl_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice display debug, ActorId);

impl ActorId {
    /// System program identifier.
    pub const SYSTEM: Self = Self(*b"geargeargeargeargeargeargeargear");

    /// Generates `ActorId` from given `CodeId` and `salt`.
    pub fn generate_from_user(code_id: CodeId, salt: &[u8]) -> Self {
        const SALT: &[u8] = b"program_from_user";
        utils::hash_of_array([SALT, code_id.as_ref(), salt]).into()
    }

    /// Generates `ActorId` from given `CodeId`, `MessageId` and `salt`.
    pub fn generate_from_program(code_id: CodeId, salt: &[u8], message_id: MessageId) -> Self {
        //TODO: consider to move `message_id` to first param
        const SALT: &[u8] = b"program_from_wasm";
        utils::hash_of_array([SALT, message_id.as_ref(), code_id.as_ref(), salt]).into()
    }

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

utils::impl_primitive!(new zero into_bytes from_u64 from_h256 display debug, MessageId);

impl MessageId {
    /// Generates `MessageId` for non-program outgoing message.
    pub fn generate_from_user(
        block_number: u32,
        user_id: ProgramId,
        local_nonce: u128,
    ) -> MessageId {
        const SALT: &[u8] = b"external";
        utils::hash_of_array([
            SALT,
            &block_number.to_le_bytes(),
            user_id.as_ref(),
            &local_nonce.to_le_bytes(),
        ])
        .into()
    }

    /// Generates `MessageId` for program outgoing message.
    pub fn generate_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId {
        const SALT: &[u8] = b"outgoing";
        utils::hash_of_array([SALT, origin_msg_id.as_ref(), &local_nonce.to_le_bytes()]).into()
    }

    /// Generates `MessageId` for reply message depend on status code.
    ///
    /// # SAFETY: DO NOT ADJUST REPLY MESSAGE ID GENERATION,
    /// BECAUSE AUTO-REPLY LOGIC DEPENDS ON PRE-DEFINED REPLY ID.
    pub fn generate_reply(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"reply";
        utils::hash_of_array([SALT, origin_msg_id.as_ref()]).into()
    }

    /// Generates `MessageId` for signal message depend on status code.
    pub fn generate_signal(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"signal";
        utils::hash_of_array([SALT, origin_msg_id.as_ref()]).into()
    }
}

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

utils::impl_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice display debug, CodeId);

impl CodeId {
    /// Generates `CodeId` from given code.
    pub fn generate(code: &[u8]) -> Self {
        utils::hash(code).into()
    }
}

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

utils::impl_primitive!(new zero into_bytes from_u64 from_h256 display debug, ReservationId);

impl ReservationId {
    /// Generates `ReservationId` from given message and nonce.
    pub fn generate(msg_id: MessageId, nonce: u64) -> Self {
        const SALT: &[u8] = b"reservation";
        utils::hash_of_array([SALT, msg_id.as_ref(), &nonce.to_le_bytes()]).into()
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use crate::*;
    use alloc::format;

    #[test]
    fn formatting_test() {
        let code_id = CodeId::generate(&[0, 1, 2]);
        let id = ProgramId::generate_from_user(code_id, &[2, 1, 0]);

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
        // Alternate formatter.
        assert_eq!(
            format!("{id:#}"),
            "ActorId(0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e)"
        );
        // Alternate formatter with precision 2.
        assert_eq!(format!("{id:#.2}"), "ActorId(0x6a51..ed3e)");
    }
}
