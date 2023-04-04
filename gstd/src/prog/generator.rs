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

//! Program generation module

use crate::{
    async_runtime::signals,
    common::errors::Result,
    msg::{CodecCreateProgramFuture, CreateProgramFuture},
    prelude::convert::AsRef,
    prog, ActorId, CodeId, MessageId,
};
use gstd_codegen::wait_create_program_for_reply;
use scale_info::scale::{alloc::vec::Vec, Decode};

/// Helper to create programs without setting the salt manually.
pub struct ProgramGenerator(u64);

// The only existing instance since there is no public ways to construct it.
static mut PROGRAM_GENERATOR: ProgramGenerator = ProgramGenerator(0);

impl ProgramGenerator {
    // Prefix for not crossing with the user salt.
    const UNIQUE_KEY: [u8; 14] = *b"salt_generator";

    /// Return the salt needed to create a new program.
    ///
    /// Salt is an arbitrary byte sequence unique after every function call.
    ///
    /// # Examples
    ///
    /// ```
    /// use gstd::prog::ProgramGenerator;
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let salt = ProgramGenerator::get_salt();
    /// }
    /// ```
    pub fn get_salt() -> Vec<u8> {
        // Provide salt uniqueness across all programs from other messages.
        let message_id = crate::msg::id();

        let creator_nonce;
        unsafe {
            creator_nonce = PROGRAM_GENERATOR.0.to_be_bytes();
            PROGRAM_GENERATOR.0 = PROGRAM_GENERATOR.0.saturating_add(1);
        }

        [&Self::UNIQUE_KEY, message_id.as_ref(), &creator_nonce].concat()
    }

    /// Create a new program from the already existing on-chain code identified
    /// by [`CodeId`].
    ///
    /// The function returns an initial message identifier and a newly created
    /// program identifier.
    ///
    /// The first argument is the code identifier (see [`CodeId`] for details).
    /// The second and third arguments are the initialization message's payload
    /// and the value to be transferred to the new program.
    ///
    /// # Examples
    ///
    /// Create a new program from the provided code identifier:
    ///
    /// ```
    /// use gstd::{msg, prog::ProgramGenerator, CodeId};
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let code_id: CodeId = msg::load().expect("Unable to load");
    ///     let (init_message_id, new_program_id) =
    ///         ProgramGenerator::create_program(code_id, b"INIT", 0)
    ///             .expect("Unable to create a program");
    ///     msg::send_bytes(new_program_id, b"PING", 0).expect("Unable to send");
    /// }
    /// ```
    #[wait_create_program_for_reply(Self)]
    pub fn create_program(
        code_id: CodeId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(MessageId, ActorId)> {
        Self::create_program_delayed(code_id, payload, value, 0)
    }

    /// Same as [`create_program`](Self::create_program), but creates a new
    /// program after the `delay` expressed in block count.
    pub fn create_program_delayed(
        code_id: CodeId,
        payload: impl AsRef<[u8]>,
        value: u128,
        delay: u32,
    ) -> Result<(MessageId, ActorId)> {
        prog::create_program_delayed(code_id, Self::get_salt(), payload, value, delay)
    }

    /// Same as [`create_program`](Self::create_program), but with an explicit
    /// gas limit.
    #[wait_create_program_for_reply(Self)]
    pub fn create_program_with_gas(
        code_id: CodeId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ActorId)> {
        Self::create_program_with_gas_delayed(code_id, payload, gas_limit, value, 0)
    }

    /// Same as [`create_program_with_gas`](Self::create_program_with_gas), but
    /// creates a new program after the `delay` expressed in block count.
    pub fn create_program_with_gas_delayed(
        code_id: CodeId,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
        delay: u32,
    ) -> Result<(MessageId, ActorId)> {
        prog::create_program_with_gas_delayed(
            code_id,
            Self::get_salt(),
            payload,
            gas_limit,
            value,
            delay,
        )
    }
}
