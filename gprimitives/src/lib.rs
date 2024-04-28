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

use blake2::{digest::typenum::U32, Blake2b, Digest};
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

/// BLAKE2b-256 hasher state.
type Blake2b256 = Blake2b<U32>;

fn hash(data: &[u8]) -> [u8; 32] {
    let mut ctx = Blake2b256::new();
    ctx.update(data);
    ctx.finalize().into()
}

fn hash_of_array<T: AsRef<[u8]>, const N: usize>(array: [T; N]) -> [u8; 32] {
    let mut ctx = Blake2b256::new();
    for data in array {
        ctx.update(data);
    }
    ctx.finalize().into()
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
}

macro_rules! declare_primitive {
    (@new $ty:ty) => {
        impl $ty {
            #[doc = concat!("Creates a new `", stringify!($ty), "` from a 32-byte array.")]
            pub const fn new(array: [u8; 32]) -> Self {
                Self(array)
            }
        }
    };
    (@zero $ty:ty) => {
        impl $ty {
            #[doc = concat!("Creates a new zero `", stringify!($ty), "`.")]
            pub const fn zero() -> Self {
                Self([0; 32])
            }

            #[doc = concat!("Checks whether `", stringify!($ty), "` is zero.")]
            pub fn is_zero(&self) -> bool {
                self == &Self::zero()
            }
        }
    };
    (@into_bytes $ty:ty) => {
        impl $ty {
            #[doc = concat!("Returns `", stringify!($ty), "`as bytes array.")]
            pub fn into_bytes(self) -> [u8; 32] {
                self.0
            }
        }
    };
    (@from_u64 $ty:ty) => {
        impl From<u64> for $ty {
            fn from(value: u64) -> Self {
                let mut id = Self::zero();
                id.0[..8].copy_from_slice(&value.to_le_bytes()[..]);
                id
            }
        }
    };
    (@from_h256 $ty:ty) => {
        #[cfg(feature = "codec")]
        impl From<H256> for $ty {
            fn from(h256: H256) -> Self {
                Self::new(h256.to_fixed_bytes())
            }
        }
    };
    (@try_from_slice $ty:ty) => {
        impl TryFrom<&[u8]> for $ty {
            type Error = ConversionError;

            fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
                if slice.len() != 32 {
                    return Err(ConversionError::InvalidSliceLength);
                }

                let mut ret = Self([0; 32]);
                ret.as_mut().copy_from_slice(slice);

                Ok(ret)
            }
        }
    };
    (@display $ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                const LEN: usize = 32;
                const MEDIAN: usize = (LEN + 1) / 2;

                let mut e1 = MEDIAN;
                let mut s2 = MEDIAN;

                if let Some(precision) = f.precision() {
                    if precision < MEDIAN {
                        e1 = precision;
                        s2 = LEN - precision;
                    }
                }

                let mut out1 = [0; MEDIAN * 2];
                let mut out2 = [0; MEDIAN * 2];

                let _ = hex::encode_to_slice(&self.0[..e1], &mut out1[..e1 * 2]);
                let _ = hex::encode_to_slice(&self.0[s2..], &mut out2[..(LEN - s2) * 2]);

                let p1 = unsafe { str::from_utf8_unchecked(&out1[..e1 * 2]) };
                let p2 = unsafe { str::from_utf8_unchecked(&out2[..(LEN - s2) * 2]) };
                let sep = e1.ne(&s2).then_some("..").unwrap_or_default();

                if f.alternate() {
                    write!(f, "{}(0x{p1}{sep}{p2})", stringify!($ty))
                } else {
                    write!(f, "0x{p1}{sep}{p2}")
                }
            }
        }
    };
    (@debug $ty:ty) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }
    };
    ($($feature:ident)*, $ty:ty) => {
        $(
            declare_primitive!(@$feature $ty);
        )*
    };
}

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

declare_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice display debug, ActorId);

impl ActorId {
    /// System program identifier.
    pub const SYSTEM: Self = Self(*b"geargeargeargeargeargeargeargear");

    /// Generates `ActorId` from given `CodeId` and `salt`.
    pub fn generate_from_user(code_id: CodeId, salt: &[u8]) -> Self {
        const SALT: &[u8] = b"program_from_user";
        hash_of_array([SALT, code_id.as_ref(), salt]).into()
    }

    /// Generates `ActorId` from given `CodeId`, `MessageId` and `salt`.
    pub fn generate_from_program(code_id: CodeId, salt: &[u8], message_id: MessageId) -> Self {
        //TODO: consider to move `message_id` to first param
        const SALT: &[u8] = b"program_from_wasm";
        hash_of_array([SALT, message_id.as_ref(), code_id.as_ref(), salt]).into()
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
            let buf = gear_ss58::decode(s.as_bytes(), 32)
                .map_err(|_| ConversionError::InvalidSs58Address)?;
            actor_id.0[..].copy_from_slice(&buf);
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

declare_primitive!(new zero into_bytes from_u64 from_h256 display debug, MessageId);

impl MessageId {
    /// Generates `MessageId` for non-program outgoing message.
    pub fn generate_from_user(
        block_number: u32,
        user_id: ProgramId,
        local_nonce: u128,
    ) -> MessageId {
        const SALT: &[u8] = b"external";
        hash_of_array([
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
        hash_of_array([SALT, origin_msg_id.as_ref(), &local_nonce.to_le_bytes()]).into()
    }

    /// Generates `MessageId` for reply message depend on status code.
    ///
    /// # SAFETY: DO NOT ADJUST REPLY MESSAGE ID GENERATION,
    /// BECAUSE AUTO-REPLY LOGIC DEPENDS ON PRE-DEFINED REPLY ID.
    pub fn generate_reply(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"reply";
        hash_of_array([SALT, origin_msg_id.as_ref()]).into()
    }

    /// Generates `MessageId` for signal message depend on status code.
    pub fn generate_signal(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"signal";
        hash_of_array([SALT, origin_msg_id.as_ref()]).into()
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

declare_primitive!(new zero into_bytes from_u64 from_h256 try_from_slice display debug, CodeId);

impl CodeId {
    /// Generates `CodeId` from given code.
    pub fn generate(code: &[u8]) -> Self {
        hash(code).into()
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

declare_primitive!(new zero into_bytes from_u64 from_h256 display debug, ReservationId);

impl ReservationId {
    /// Generates `ReservationId` from given message and nonce.
    pub fn generate(msg_id: MessageId, nonce: u64) -> Self {
        const SALT: &[u8] = b"reservation";
        hash_of_array([SALT, msg_id.as_ref(), &nonce.to_le_bytes()]).into()
    }
}
