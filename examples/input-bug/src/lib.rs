// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

extern crate alloc;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use super::WASM_BINARY;
    use alloc::vec::Vec;
    use gtest::{constants, Program, System};

    #[test]
    #[should_panic]
    fn basic_test() {
        let system = System::new();
        system.init_logger();

        let alice = constants::DEFAULT_USER_ALICE;

        let prog = Program::from_binary_with_id(&system, 142, WASM_BINARY);

        // Init program
        let init_mid = prog.send(alice, *b"init");
        let res = system.run_next_block();
        assert!(res.succeed.contains(&init_mid));

        // Handle
        let payload = [1u8; 49];
        let handle_mid = prog.send(alice, payload);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&handle_mid));

        let mut log = res
            .log()
            .iter()
            .filter(|log| log.reply_to().is_none())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(log.len(), 1);

        let sent_to_user_msg = log.pop().expect("checked");
        assert!(!sent_to_user_msg.payload().is_empty())
    }
}
