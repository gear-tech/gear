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

use crate::{common::errors::Result, prog, ActorId, CodeHash};
use codec::alloc::vec::Vec;

/// `ProgramGenerator` allows you to create programs
/// without need to set the salt manually.
pub struct ProgramGenerator(u64);

// The only existing instance since there is no public ways to construct it.
static mut PROGRAM_GENERATOR: ProgramGenerator = ProgramGenerator(0);

impl ProgramGenerator {
    pub fn get_salt() -> Vec<u8> {
        // Prefix for not crossing with the user salt.
        let unique_key = b"salt_generator";
        // Provide salt uniqueness across all programs from other messages.
        let message_id = crate::msg::id();

        let creator_nonce;
        unsafe {
            creator_nonce = PROGRAM_GENERATOR.0.to_be_bytes();
            PROGRAM_GENERATOR.0 = PROGRAM_GENERATOR.0.saturating_add(1);
        }

        [unique_key, message_id.as_ref(), &creator_nonce].concat()
    }

    pub fn create_program_with_gas(
        code_hash: CodeHash,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
    ) -> Result<ActorId> {
        prog::create_program_with_gas(code_hash, Self::get_salt(), payload, gas_limit, value)
    }

    pub fn create_program_with_gas_delayed(
        code_hash: CodeHash,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
        delay: u32,
    ) -> Result<ActorId> {
        prog::create_program_with_gas_delayed(
            code_hash,
            Self::get_salt(),
            payload,
            gas_limit,
            value,
            delay,
        )
    }

    pub fn create_program(
        code_hash: CodeHash,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<ActorId> {
        prog::create_program(code_hash, Self::get_salt(), payload, value)
    }

    pub fn create_program_delayed(
        code_hash: CodeHash,
        payload: impl AsRef<[u8]>,
        value: u128,
        delay: u32,
    ) -> Result<ActorId> {
        prog::create_program_delayed(code_hash, Self::get_salt(), payload, value, delay)
    }
}
