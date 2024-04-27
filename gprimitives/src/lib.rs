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

use derive_more::{AsMut, AsRef, Display, From, Into};
#[cfg(feature = "codec")]
use {
    primitive_types::H256,
    scale_info::{
        scale::{self, Decode, Encode, MaxEncodedLen},
        TypeInfo,
    },
};

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
}

macro_rules! declare_primitive {
    (@new $ty:ty) => {
        impl $ty {
            #[doc = concat!("Create a new `", stringify!($ty), "` from a 32-byte array.")]
            pub const fn new(array: [u8; 32]) -> Self {
                Self(array)
            }
        }
    };
    (@zero $ty:ty) => {
        impl $ty {
            #[doc = concat!("Create a new zero `", stringify!($ty), "`.")]
            pub const fn zero() -> Self {
                Self([0; 32])
            }

            #[doc = concat!("Check whether `", stringify!($ty), "` is zero.")]
            pub fn is_zero(&self) -> bool {
                self == &Self::zero()
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
    ($($feature:ident)*, $ty:ty) => {
        $(
            declare_primitive!(@$feature $ty);
        )*
    };
}

/// Program (actor) identifier.
///
/// Gear allows user and program interactions via messages. Source and target
/// program as well as user are represented by 256-bit identifier `ActorId`
/// struct. The source `ActorId` for a message being processed can be obtained
/// using `msg::source()` function. Also, each send function has a target
/// `ActorId` as one of the arguments.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut,
)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct ActorId([u8; 32]);

declare_primitive!(new zero from_h256 try_from_slice, ActorId);

impl From<u64> for ActorId {
    fn from(value: u64) -> Self {
        let mut actor_id = Self::zero();
        actor_id.0[..8].copy_from_slice(&value.to_le_bytes()[..]);

        actor_id
    }
}

/// Message identifier.
///
/// Gear allows users and program interactions via messages. Each message has
/// its own unique 256-bit id. This id is represented via the `MessageId`
/// struct. The message identifier can be obtained for the currently processed
/// message using the `msg::id()` function. Also, each send and reply functions
/// return a message identifier.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut,
)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct MessageId([u8; 32]);

declare_primitive!(new zero from_h256, MessageId);

/// Code identifier.
///
/// This identifier can be obtained as a result of executing the
/// `gear.uploadCode` extrinsic. Actually, the code identifier is the Blake2
/// hash of the Wasm binary code blob.
///
/// Code identifier is required when creating programs from programs (see `prog`
/// module for details).
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut,
)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct CodeId([u8; 32]);

declare_primitive!(new from_h256 try_from_slice, CodeId);

/// Reservation identifier.
///
/// The identifier is used to reserve and unreserve gas amount for program
/// execution later.
#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, From, Into, AsRef, AsMut,
)]
#[as_ref(forward)]
#[as_mut(forward)]
#[cfg_attr(feature = "codec", derive(TypeInfo, Encode, Decode, MaxEncodedLen), codec(crate = scale))]
pub struct ReservationId([u8; 32]);

