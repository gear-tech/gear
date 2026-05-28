// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
    use alloc::vec::Vec;
    use gstd::ActorId;
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn auto_reply_received() {
        let system = System::new();
        system.init_logger();

        let prog1 = Program::current(&system);
        let prog1_id = ActorId::try_from(prog1.id().as_ref()).unwrap();

        let from = DEFAULT_USER_ALICE;

        // Init Program-1
        let init_msg1 = prog1.send(from, ActorId::zero());

        // Init Program-2 with Program-1 as destination
        let prog2 = Program::current(&system);
        let init_msg2 = prog2.send(from, prog1_id);

        // Send a message from Program-2 to Program-1
        let msg3 = prog2.send_bytes(from, b"Let's go!");

        let res = system.run_next_block();
        for msg in [init_msg1, init_msg2, msg3] {
            assert!(res.succeed.contains(&msg));
        }

        // Check whether the auto-reply was received
        let reply_received: bool = prog2
            .read_state(Vec::<u8>::default())
            .expect("Failed to read state");
        assert!(reply_received);
    }
}
