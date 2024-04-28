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

use blake2::{digest::typenum::U32, Blake2b, Digest};
pub use gprimitives::{ActorId, CodeId, MessageId, ProgramId, ReservationId};

/// Hash length used in gear protocol.
pub const HASH_LENGTH: usize = 32;

/// Hash type used in gear protocol.
pub type Hash = [u8; HASH_LENGTH];

/// BLAKE2b-256 hasher state.
type Blake2b256 = Blake2b<U32>;

/// Creates a unique identifier by passing given argument to blake2b hash-function.
///
/// # SAFETY: DO NOT ADJUST HASH FUNCTION, BECAUSE MESSAGE ID IS SENSITIVE FOR IT.
pub fn hash(data: &[u8]) -> Hash {
    let mut ctx = Blake2b256::new();
    ctx.update(data);
    ctx.finalize().into()
}

/// Creates a unique identifier by passing given argument to blake2b hash-function.
///
/// # SAFETY: DO NOT ADJUST HASH FUNCTION, BECAUSE MESSAGE ID IS SENSITIVE FOR IT.
pub fn hash_of_array<T: AsRef<[u8]>, const N: usize>(array: [T; N]) -> Hash {
    let mut ctx = Blake2b256::new();
    for data in array {
        ctx.update(data);
    }
    ctx.finalize().into()
}

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

#[test]
fn formatting_test() {
    use alloc::format;

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
        "ProgramId(0x6a519a19ffdfd8f45c310b44aecf156b080c713bf841a8cb695b0ea5f765ed3e)"
    );
    // Alternate formatter with precision 2.
    assert_eq!(format!("{id:#.2}"), "ProgramId(0x6a51..ed3e)");
}
