// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![no_std]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(target_arch = "wasm32")]
#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn ctor_and_dtor_work() {
        let system = System::new();
        let prog = Program::current(&system);

        let init_msg = prog.send_bytes(DEFAULT_USER_ALICE, []);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&init_msg));

        let handle_msg = prog.send_bytes(DEFAULT_USER_ALICE, []);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&handle_msg));
    }
}
