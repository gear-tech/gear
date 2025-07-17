// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use blake2::{Blake2b, Digest, digest::typenum::U32};
pub use gprimitives::{ActorId, CodeId, MessageId, ReservationId};

/// BLAKE2b-256 hasher state.
type Blake2b256 = Blake2b<U32>;

/// Creates a unique identifier by passing given argument to blake2b hash-function.
///
/// # SAFETY: DO NOT ADJUST HASH FUNCTION, BECAUSE MESSAGE ID IS SENSITIVE FOR IT.
pub fn hash(data: &[u8]) -> [u8; 32] {
    let mut ctx = Blake2b256::new();
    ctx.update(data);
    ctx.finalize().into()
}

/// Creates a unique identifier by passing given argument to blake2b hash-function.
///
/// # SAFETY: DO NOT ADJUST HASH FUNCTION, BECAUSE MESSAGE ID IS SENSITIVE FOR IT.
pub fn hash_of_array<T: AsRef<[u8]>, const N: usize>(array: [T; N]) -> [u8; 32] {
    let mut ctx = Blake2b256::new();
    for data in array {
        ctx.update(data);
    }
    ctx.finalize().into()
}

pub mod prelude {
    //! The purpose of this module is to make it easier to import `gprimitives` extensions.
    use super::*;

    mod private {
        use super::*;

        pub trait Sealed {}

        impl Sealed for ActorId {}
        impl Sealed for CodeId {}
        impl Sealed for MessageId {}
        impl Sealed for ReservationId {}
    }

    /// Program (actor) identifier extension.
    pub trait ActorIdExt: private::Sealed {
        /// System program identifier.
        const SYSTEM: Self;

        /// Generates `ActorId` from given `CodeId` and `salt`.
        fn generate_from_user(code_id: CodeId, salt: &[u8]) -> Self;

        /// Generates `ActorId` from given `MessageId`, `CodeId` and `salt`.
        fn generate_from_program(message_id: MessageId, code_id: CodeId, salt: &[u8]) -> Self;
    }

    impl ActorIdExt for ActorId {
        const SYSTEM: Self = Self::new(*b"geargeargeargeargeargeargeargear");

        fn generate_from_user(code_id: CodeId, salt: &[u8]) -> Self {
            const SALT: &[u8] = b"program_from_user";
            hash_of_array([SALT, code_id.as_ref(), salt]).into()
        }

        fn generate_from_program(message_id: MessageId, code_id: CodeId, salt: &[u8]) -> Self {
            const SALT: &[u8] = b"program_from_wasm";
            hash_of_array([SALT, message_id.as_ref(), code_id.as_ref(), salt]).into()
        }
    }

    /// Message identifier extension.
    pub trait MessageIdExt: private::Sealed {
        /// Generates `MessageId` for non-program outgoing message.
        fn generate_from_user(block_number: u32, user_id: ActorId, local_nonce: u128) -> MessageId;

        /// Generates `MessageId` for program outgoing message.
        fn generate_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId;

        /// Generates `MessageId` for reply message depend on status code.
        ///
        /// # SAFETY: DO NOT ADJUST REPLY MESSAGE ID GENERATION,
        /// BECAUSE AUTO-REPLY LOGIC DEPENDS ON PRE-DEFINED REPLY ID.
        fn generate_reply(origin_msg_id: MessageId) -> MessageId;

        /// Generates `MessageId` for signal message depend on status code.
        fn generate_signal(origin_msg_id: MessageId) -> MessageId;
    }

    impl MessageIdExt for MessageId {
        fn generate_from_user(block_number: u32, user_id: ActorId, local_nonce: u128) -> MessageId {
            const SALT: &[u8] = b"external";
            hash_of_array([
                SALT,
                &block_number.to_le_bytes(),
                user_id.as_ref(),
                &local_nonce.to_le_bytes(),
            ])
            .into()
        }

        fn generate_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId {
            const SALT: &[u8] = b"outgoing";
            hash_of_array([SALT, origin_msg_id.as_ref(), &local_nonce.to_le_bytes()]).into()
        }

        fn generate_reply(origin_msg_id: MessageId) -> MessageId {
            const SALT: &[u8] = b"reply";
            hash_of_array([SALT, origin_msg_id.as_ref()]).into()
        }

        fn generate_signal(origin_msg_id: MessageId) -> MessageId {
            const SALT: &[u8] = b"signal";
            hash_of_array([SALT, origin_msg_id.as_ref()]).into()
        }
    }

    /// Code identifier extension.
    pub trait CodeIdExt: private::Sealed {
        /// Generates `CodeId` from given code.
        fn generate(code: &[u8]) -> Self;
    }

    impl CodeIdExt for CodeId {
        fn generate(code: &[u8]) -> Self {
            hash(code).into()
        }
    }

    /// Reservation identifier extension.
    pub trait ReservationIdExt: private::Sealed {
        /// Generates `ReservationId` from given message and nonce.
        fn generate(msg_id: MessageId, nonce: u64) -> Self;
    }

    impl ReservationIdExt for ReservationId {
        fn generate(msg_id: MessageId, nonce: u64) -> Self {
            const SALT: &[u8] = b"reservation";
            hash_of_array([SALT, msg_id.as_ref(), &nonce.to_le_bytes()]).into()
        }
    }
}
