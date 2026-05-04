// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

#![no_std]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
pub mod wasm;

pub mod constants;
pub mod data_access;

#[cfg(test)]
mod tests {
    use crate::data_access::DataAccess;
    use gtest::{Log, Program, System};

    const USER_ID: u64 = gtest::constants::DEFAULT_USER_ALICE;

    use proptest::{
        arbitrary::any, collection::vec, proptest, test_runner::Config as ProptestConfig,
    };
    // Testing random access to data section
    proptest! {
        #![proptest_config(ProptestConfig {
                    cases: 200, // Set the number of test cases to run
                    .. ProptestConfig::default()
                })]
        #[test]
        fn test_big_data_section(payload in vec(any::<u8>(), 2..100)){
            let sys = System::new();
            sys.init_logger();

            let prog = Program::current_opt(&sys);
            sys.mint_to(gtest::constants::DEFAULT_USER_ALICE, 100000000000);

            let expected_value = DataAccess::from_payload(&payload).expect("").constant();

            let message_id = prog.send_bytes(USER_ID, payload);
            let block_run_result = sys.run_next_block();

            let log = Log::builder()
                .source(prog.id())
                .dest(USER_ID)
                .payload_bytes(expected_value.to_be_bytes());

            assert!(block_run_result.succeed.contains(&message_id));
            assert!(block_run_result.contains(&log));
        }
    }
}
