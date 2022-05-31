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

use crate::{ActorId, CodeHash};
use codec::alloc::vec::Vec;

use super::create_program_with_gas;

static mut SALT_GENERATOR_NONCE: u32 = 0;

fn get_salt_generator_nonce() -> u32 {
    let result;
    unsafe {
        result = SALT_GENERATOR_NONCE;
        SALT_GENERATOR_NONCE = SALT_GENERATOR_NONCE.saturating_add(1);
    }
    result
}

pub struct ProgramGenerator {
    /// number unique for every creator.
    creator_nonce: u32,
    /// number unique for salt in this creator.
    salt_nonce: u32,
}

impl ProgramGenerator {
    pub fn new() -> Self {
        ProgramGenerator {
            creator_nonce: get_salt_generator_nonce(),
            salt_nonce: 0,
        }
    }

    fn get_salt(&mut self) -> Vec<u8> {
        // Prefix for not crossing with the user salt.
        let unique_key = b"unique_key: c5755111a6dc6b7498a5";
        // Provide salt uniqueness across all programs from other messages.
        let message_id = crate::msg::id();
        let creator_nonce = &self.creator_nonce.to_be_bytes();
        let salt_nonce = &self.salt_nonce.to_be_bytes();

        [unique_key, message_id.as_ref(), creator_nonce, salt_nonce].concat()
    }

    pub fn create_program_with_gas<T: AsRef<[u8]>>(
        &mut self,
        code_hash: CodeHash,
        payload: T,
        gas_limit: u64,
        value: u128,
    ) -> ActorId {
        create_program_with_gas(code_hash, self.get_salt(), payload, gas_limit, value)
    }
}

impl Default for ProgramGenerator {
    fn default() -> Self {
        Self::new()
    }
}
