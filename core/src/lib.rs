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

//! Gear core.
//!
//! This library provides a runner for dealing with multiple little programs exchanging messages in a deterministic manner.
//! To be used primary in Gear Substrate node implementation, but it is not limited to that.
#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

extern crate alloc;

pub use gprimitives as primitives;

// This allows all casts from u32 into usize be safe.
const _: () = assert!(size_of::<u32>() <= size_of::<usize>());

pub mod buffer;
pub mod code;
pub mod costs;
pub mod env;
pub mod env_vars;
pub mod gas;
pub mod gas_metering;
pub mod memory;
pub mod message;
pub mod pages;
pub mod percent;
pub mod program;
pub mod reservation;
pub mod rpc;
pub mod str;
pub mod tasks;
pub mod utils {
    //! The purpose of this module is to make it easier to import `gprimitives` extensions.
    use gprimitives::{
        hashing::hash_array as hash_of_array, ActorId, CodeId, MessageId, ReservationId,
    };

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_pid_from_user(code_id: CodeId, salt: &[u8]) -> ActorId {
        const SALT: &[u8] = b"program_from_user";
        hash_of_array([SALT, code_id.as_ref(), salt]).into()
    }

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_pid_from_program(
        message_id: MessageId,
        code_id: CodeId,
        salt: &[u8],
    ) -> ActorId {
        const SALT: &[u8] = b"program_from_wasm";
        hash_of_array([SALT, message_id.as_ref(), code_id.as_ref(), salt]).into()
    }

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_mid_from_user(
        block_number: u32,
        user_id: ActorId,
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

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_mid_outgoing(origin_msg_id: MessageId, local_nonce: u32) -> MessageId {
        const SALT: &[u8] = b"outgoing";
        hash_of_array([SALT, origin_msg_id.as_ref(), &local_nonce.to_le_bytes()]).into()
    }

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_mid_reply(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"reply";
        hash_of_array([SALT, origin_msg_id.as_ref()]).into()
    }

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_mid_signal(origin_msg_id: MessageId) -> MessageId {
        const SALT: &[u8] = b"signal";
        hash_of_array([SALT, origin_msg_id.as_ref()]).into()
    }

    /// TO BE REFACTORED IN THE NEXT COMMIT.
    pub fn generate_rid(msg_id: MessageId, nonce: u64) -> ReservationId {
        const SALT: &[u8] = b"reservation";
        hash_of_array([SALT, msg_id.as_ref(), &nonce.to_le_bytes()]).into()
    }
}
