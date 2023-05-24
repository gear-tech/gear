// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use blake2_rfc::blake2b;
use core::convert::TryInto;

const HASH_LENGTH: usize = 32;
type Hash = [u8; HASH_LENGTH];

/// Creates a unique identifier by passing given argument to blake2b hash-function.
fn hash(argument: &[u8]) -> Hash {
    let blake2b_hash = blake2b::blake2b(HASH_LENGTH, &[], argument);

    blake2b_hash
        .as_bytes()
        .try_into()
        .expect("we set hash len; qed")
}

/// Declares data type for storing any kind of id for gear-core,
/// which stores 32 bytes under the hood.
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
            scale_info::scale::Decode,
            scale_info::scale::Encode,
            parity_scale_codec::MaxEncodedLen,
            derive_more::From,
            scale_info::TypeInfo,
        )]
        pub struct $name(Hash);

        impl $name {
            /// Returns id as bytes array.
            pub fn into_bytes(self) -> Hash {
                self.0
            }
        }

        impl From<$name> for Hash {
            fn from(val: $name) -> Hash {
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
                if slice.len() != HASH_LENGTH {
                    panic!("Identifier must be 32 length");
                }

                let mut arr: Hash = Default::default();
                arr[..].copy_from_slice(slice);

                Self(arr)
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
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
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                core::fmt::Display::fmt(self, f)
            }
        }
    };
}

declare_id!(CodeId: "Code identifier");

impl CodeId {
    /// Generate CodeId from given code
    pub fn generate(code: &[u8]) -> Self {
        hash(code).into()
    }
}

declare_id!(MessageId: "Message identifier");

impl MessageId {
    /// Generate MessageId for non-program outgoing message
    pub fn generate_from_user(
        block_number: u32,
        user_id: ProgramId,
        local_nonce: u128,
    ) -> MessageId {
        const SALT: &[u8] = b"external";

        let argument = [
            SALT,
            &block_number.to_le_bytes(),
            user_id.as_ref(),
            &local_nonce.to_le_bytes(),
        ]
        .concat();
        hash(&argument).into()
    }

    /// Generate MessageId for program outgoing message
    pub fn generate_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId {
        const SALT: &[u8] = b"outgoing";

        let argument = [SALT, origin_msg_id.as_ref(), &local_nonce.to_le_bytes()].concat();
        hash(&argument).into()
    }

    /// Generate MessageId for reply message depend on status code
    pub fn generate_reply(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"reply";

        let argument = [SALT, origin_msg_id.as_ref()].concat();
        hash(&argument).into()
    }

    /// Generate MessageId for signal message depend on status code
    pub fn generate_signal(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"signal";

        let argument = [SALT, origin_msg_id.as_ref()].concat();
        hash(&argument).into()
    }
}

declare_id!(ProgramId: "Program identifier");

impl ProgramId {
    /// System program ID
    pub const SYSTEM: Self = Self(*b"geargeargeargeargeargeargeargear");

    /// Generate ProgramId from given CodeId and salt
    pub fn generate(code_id: CodeId, salt: &[u8]) -> Self {
        const SALT: &[u8] = b"program";

        let argument = [SALT, code_id.as_ref(), salt].concat();
        hash(&argument).into()
    }
}

declare_id!(ReservationId: "Reservation identifier");

impl ReservationId {
    /// Create a new reservation ID
    pub fn generate(msg_id: MessageId, nonce: u64) -> Self {
        const SALT: &[u8] = b"reservation";

        let argument = [SALT, msg_id.as_ref(), &nonce.to_le_bytes()].concat();
        hash(&argument).into()
    }
}

#[test]
fn formatting_test() {
    use alloc::format;

    let code_id = CodeId::generate(&[0, 1, 2]);
    let id = ProgramId::generate(code_id, &[2, 1, 0]);

    // `Debug`/`Display`.
    assert_eq!(
        format!("{id:?}"),
        "0x227e53192dc14699539c44608810a8202d6a2bee92078e6913b1bdf38925fa67"
    );
    // `Debug`/`Display` with precision 0.
    assert_eq!(format!("{id:.0?}"), "0x..");
    // `Debug`/`Display` with precision 1.
    assert_eq!(format!("{id:.1?}"), "0x22..67");
    // `Debug`/`Display` with precision 2.
    assert_eq!(format!("{id:.2?}"), "0x227e..fa67");
    // `Debug`/`Display` with precision 4.
    assert_eq!(format!("{id:.4?}"), "0x227e5319..8925fa67");
    // `Debug`/`Display` with precision 15.
    assert_eq!(
        format!("{id:.15?}"),
        "0x227e53192dc14699539c44608810a8..6a2bee92078e6913b1bdf38925fa67"
    );
    // `Debug`/`Display` with precision 30 (the same for any case >= 16).
    assert_eq!(
        format!("{id:.30?}"),
        "0x227e53192dc14699539c44608810a8202d6a2bee92078e6913b1bdf38925fa67"
    );
    // Alternate formatter.
    assert_eq!(
        format!("{id:#}"),
        "ProgramId(0x227e53192dc14699539c44608810a8202d6a2bee92078e6913b1bdf38925fa67)"
    );
    // Alternate formatter with precision 2.
    assert_eq!(format!("{id:#.2}"), "ProgramId(0x227e..fa67)");
}
