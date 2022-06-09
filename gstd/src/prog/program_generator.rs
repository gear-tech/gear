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

use crate::{common::errors::Result, ActorId, CodeHash};
use codec::alloc::vec::Vec;

use super::{create_program, create_program_with_gas};

pub struct ProgramGenerator(u64);

// The only existing instance since there is no public ways to construct it.
static mut PROGRAM_GENERATOR: ProgramGenerator = ProgramGenerator(0);

impl ProgramGenerator {
    fn get_salt(&mut self) -> Vec<u8> {
        // Prefix for not crossing with the user salt.
        let unique_key = b"unique_key: c5755111a6dc6b7498a5";
        // Provide salt uniqueness across all programs from other messages.
        let message_id = crate::msg::id();
        let creator_nonce = &self.0.to_be_bytes();
        self.0 = self.0.saturating_add(1);

        [unique_key, message_id.as_ref(), creator_nonce].concat()
    }

    pub fn create_program_with_gas<T: AsRef<[u8]>>(
        code_hash: CodeHash,
        payload: T,
        gas_limit: Option<u64>,
        value: u128,
    ) -> Result<ActorId> {
        let salt = unsafe { PROGRAM_GENERATOR.get_salt() };

        if let Some(gas_limit) = gas_limit {
            create_program_with_gas(code_hash, salt, payload, gas_limit, value)
        } else {
            create_program(code_hash, salt, payload, value)
        }
    }
}

#[cfg(test)]
mod tests {
    use codec::alloc::vec::Vec;

    use crate::prog::program_generator::PROGRAM_GENERATOR;

    #[test]
    fn salt_uniqueness_test() {
        let n = 10;
        let salts: Vec<Vec<u8>> = (0..n)
            .map(|_| unsafe { PROGRAM_GENERATOR.get_salt() })
            .collect();

        for first_salt in salts.iter() {
            for second_salt in salts.iter() {
                assert_eq!(first_salt, second_salt);
            }
        }
    }
}
