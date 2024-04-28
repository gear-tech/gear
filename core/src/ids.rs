// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

//! Base identifiers for messaging primitives.

pub use gprimitives::{
    utils::{hash, hash_of_array},
    ActorId, CodeId, MessageId, ProgramId, ReservationId,
};

/// Hash length used in gear protocol.
pub const HASH_LENGTH: usize = 32;

/// Hash type used in gear protocol.
pub type Hash = [u8; HASH_LENGTH];

/// Declares data type for storing any kind of id for gear-core,
/// which stores 32 bytes under the hood.
#[macro_export]
macro_rules! declare_id {
    ($name:ident: $doc: literal) => {
        #[doc=$doc]
        #[derive(
            Clone,
            Copy,
            Default,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Decode,
            Encode,
            parity_scale_codec::MaxEncodedLen,
            derive_more::From,
            TypeInfo,
        )]
        pub struct $name($crate::ids::Hash);

        impl $name {
            /// Creates new id.
            ///
            /// Never use it in production!
            pub const fn test_new(hash: $crate::ids::Hash) -> Self {
                Self(hash)
            }

            /// Returns id as bytes array.
            pub fn into_bytes(self) -> $crate::ids::Hash {
                self.0
            }
        }

        impl From<$name> for $crate::ids::Hash {
            fn from(val: $name) -> $crate::ids::Hash {
                val.0
            }
        }

        impl From<u64> for $name {
            fn from(v: u64) -> Self {
                let mut id = Self(Default::default());
                id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
                id
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                self.0.as_ref()
            }
        }

        impl AsMut<[u8]> for $name {
            fn as_mut(&mut self) -> &mut [u8] {
                self.0.as_mut()
            }
        }

        impl From<&[u8]> for $name {
            fn from(slice: &[u8]) -> Self {
                if slice.len() != $crate::ids::HASH_LENGTH {
                    panic!("Identifier must be 32 length");
                }

                let mut arr: $crate::ids::Hash = Default::default();
                arr[..].copy_from_slice(slice);

                Self(arr)
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let len = self.0.len();
                let median = (len + 1) / 2;

                let mut e1 = median;
                let mut s2 = median;

                if let Some(precision) = f.precision() {
                    if precision < median {
                        e1 = precision;
                        s2 = len - precision;
                    }
                }

                let p1 = hex::encode(&self.0[..e1]);
                let p2 = hex::encode(&self.0[s2..]);
                let sep = e1.ne(&s2).then_some("..").unwrap_or_default();

                if f.alternate() {
                    write!(f, "{}(0x{p1}{sep}{p2})", stringify!($name))
                } else {
                    write!(f, "0x{p1}{sep}{p2}")
                }
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(self, f)
            }
        }
    };
}
