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
    use gear_core::ids::prelude::MessageIdExt;
    use gstd::{ActorId, MessageId};
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn payload_received() {
        let system = System::new();
        system.init_logger();

        let panicking_prog = Program::current(&system);
        let sending_prog = Program::current(&system);

        // init
        let panicking_init_id = panicking_prog.send(DEFAULT_USER_ALICE, ActorId::zero());
        let sending_init_id = sending_prog.send(DEFAULT_USER_ALICE, panicking_prog.id());

        let res = system.run_next_block();
        assert!(res.succeed.contains(&panicking_init_id));
        assert!(res.succeed.contains(&sending_init_id));

        // handle
        let sending_handle_id = sending_prog.send_bytes(DEFAULT_USER_ALICE, []);

        let mut res = system.run_next_block();
        assert!(res.succeed.contains(&sending_handle_id));
        assert_eq!(res.failed.len(), 1);
        let panicked_msg_id = res.failed.pop_first().unwrap();
        let reply_msg_id = MessageId::generate_reply(panicked_msg_id);
        assert!(res.succeed.contains(&reply_msg_id));
    }
}
