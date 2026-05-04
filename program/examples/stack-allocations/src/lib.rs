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

//! This program recursively calls payload stack allocated read or load,
//! depends on actions vector, which is set in init.
//! For each recursion step we call check_sum, which is sum of all payload bytes.
//! Then reply summary check_sum back to source account.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

extern crate alloc;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};

#[derive(Encode, Decode)]
pub struct InitConfig {
    actions: Vec<Action>,
}

#[derive(Encode, Decode, Clone)]
pub enum Action {
    Read,
    Load,
}

const HANDLE_DATA_SIZE: usize = 0x100;

#[derive(Encode, Decode)]
pub struct HandleData {
    data: [u8; HANDLE_DATA_SIZE],
}

#[derive(Encode, Decode, MaxEncodedLen)]
pub struct ReplyData {
    check_sum: u32,
}

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use super::*;
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};
    use parity_scale_codec::Decode;
    use rand::{Rng, SeedableRng};

    #[test]
    fn stress() {
        use Action::*;

        const MAX_ACTIONS_AMOUNT: usize = 1000;
        const MAX_NUMBER: u8 = 255;

        const {
            // Check that check sum is less than u32::MAX
            assert!(
                MAX_ACTIONS_AMOUNT * MAX_NUMBER as usize * HANDLE_DATA_SIZE <= u32::MAX as usize
            );
            // Check that we can fit all the data in the stack (heuristic no more than 10 wasm pages)
            assert!(MAX_ACTIONS_AMOUNT * HANDLE_DATA_SIZE <= 64 * 1024 * 10);
        }

        let from = DEFAULT_USER_ALICE;
        let system = System::new();
        system.init_logger();

        let mut rng = rand_pcg::Pcg32::seed_from_u64(42);

        for _ in 0..50 {
            let program = Program::current_opt(&system);
            let mut actions = Vec::new();
            let actions_amount = rng.gen_range(1..=MAX_ACTIONS_AMOUNT);
            for _ in 0..actions_amount {
                actions.push(if rng.gen_range(0..=1) == 0 {
                    Read
                } else {
                    Load
                });
            }

            // Init program
            let msg_id = program.send(from, InitConfig { actions });
            let res = system.run_next_block();
            assert!(res.succeed.contains(&msg_id));

            let number: u8 = rng.gen_range(0..=MAX_NUMBER);
            let expected_check_sum = actions_amount * number as usize * HANDLE_DATA_SIZE;

            // Send data to handle
            program.send(
                from,
                HandleData {
                    data: [number; HANDLE_DATA_SIZE],
                },
            );
            let res = system.run_next_block();

            assert_eq!(
                expected_check_sum as u32,
                ReplyData::decode(&mut res.log()[0].payload())
                    .expect("Cannot decode reply")
                    .check_sum
            );
        }
    }
}
