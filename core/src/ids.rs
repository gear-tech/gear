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

use crate::message::ExitCode;
use alloc::vec::Vec;
use blake2_rfc::blake2b;

const HASH_LENGTH: usize = 32;
type Hash = [u8; HASH_LENGTH];

/// Creates a unique identifier by passing given argument to blake2b hash-function.
fn hash(argument: &[u8]) -> Hash {
    let mut arr: Hash = Default::default();

    let blake2b_hash = blake2b::blake2b(HASH_LENGTH, &[], argument);
    arr[..].copy_from_slice(blake2b_hash.as_bytes());

    arr
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
            codec::Decode,
            codec::Encode,
            codec::MaxEncodedLen,
            derive_more::From,
            scale_info::TypeInfo,
        )]
        pub struct $name(Hash);

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
                let mut end = self.0.len();

                if let Some(precision) = f.precision() {
                    if precision > end {
                        return Err(core::fmt::Error);
                    }

                    end = precision;
                };

                write!(f, "0x{}", hex::encode(&self.0[..end]))
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
        let unique_flag = b"from_user";

        let block_number = block_number.to_le_bytes();
        let user_id = user_id.as_ref();
        let local_nonce = local_nonce.to_le_bytes();

        let len = unique_flag.len() + block_number.len() + user_id.len() + local_nonce.len();

        let mut argument = Vec::with_capacity(len);
        argument.extend_from_slice(unique_flag);
        argument.extend(block_number);
        argument.extend_from_slice(user_id);
        argument.extend(local_nonce);

        hash(&argument).into()
    }

    /// Generate MessageId for program outgoing message
    pub fn generate_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId {
        let unique_flag = b"outgoing";

        let origin_msg_id = origin_msg_id.as_ref();
        let local_nonce = local_nonce.to_le_bytes();

        let len = unique_flag.len() + origin_msg_id.len() + local_nonce.len();

        let mut argument = Vec::with_capacity(len);
        argument.extend_from_slice(unique_flag);
        argument.extend(origin_msg_id);
        argument.extend(local_nonce);

        hash(&argument).into()
    }

    /// Generate MessageId for reply message depend on exit code
    pub fn generate_reply(origin_msg_id: MessageId, exit_code: ExitCode) -> MessageId {
        let unique_flag = b"reply";

        let origin_msg_id = origin_msg_id.as_ref();
        let exit_code = exit_code.to_le_bytes();

        let len = unique_flag.len() + origin_msg_id.len() + exit_code.len();

        let mut argument = Vec::with_capacity(len);
        argument.extend_from_slice(unique_flag);
        argument.extend(exit_code);
        argument.extend(origin_msg_id);

        hash(&argument).into()
    }
}

declare_id!(ProgramId: "Program identifier");

impl ProgramId {
    /// Generate ProgramId from given CodeId and salt
    pub fn generate(code_id: CodeId, salt: &[u8]) -> Self {
        let code_id = code_id.as_ref();

        let len = code_id.len() + salt.len();

        let mut argument = Vec::with_capacity(len);
        argument.extend_from_slice(code_id);
        argument.extend_from_slice(salt);

        hash(&argument).into()
    }
}

declare_id!(ReservationId: "Reservation identifier");

impl ReservationId {
    /// Create a new reservation ID
    pub fn generate(msg_id: MessageId, idx: u64) -> Self {
        let argument = [msg_id.as_ref(), &idx.to_le_bytes(), b"reservation_id_salt"].concat();
        hash(&argument).into()
    }
}

impl From<ReservationId> for MessageId {
    fn from(id: ReservationId) -> Self {
        Self(id.0)
    }
}
