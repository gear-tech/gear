// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
