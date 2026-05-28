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

use gstd::{ActorId, Decode, Encode};

#[cfg(not(feature = "std"))]
mod wasm;

#[derive(Debug, Encode, Decode, Clone, Copy)]
pub enum State {
    Exiting { inheritor: ActorId },
    Assertive { send_to: ActorId },
}

#[cfg(test)]
mod tests {
    use crate::State;
    use gear_core::ids::prelude::MessageIdExt;
    use gstd::MessageId;
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn payload_received() {
        let system = System::new();
        system.init_logger();

        let exiting_program = Program::current(&system);
        let assertive_program = Program::current(&system);

        // init
        let exiting_init_id = exiting_program.send(
            DEFAULT_USER_ALICE,
            State::Exiting {
                inheritor: assertive_program.id(),
            },
        );
        let assertive_init_id = assertive_program.send(
            DEFAULT_USER_ALICE,
            State::Assertive {
                send_to: exiting_program.id(),
            },
        );

        let res = system.run_next_block();
        assert!(res.succeed.contains(&exiting_init_id));
        assert!(res.succeed.contains(&assertive_init_id));

        // trigger exit
        let exiting_handle_id = exiting_program.send_bytes(DEFAULT_USER_ALICE, []);

        let res = system.run_next_block();
        assert!(res.succeed.contains(&exiting_handle_id));

        // trigger reply
        let assertive_handle_id = assertive_program.send_bytes(DEFAULT_USER_ALICE, []);

        let mut res = system.run_next_block();
        assert!(res.succeed.contains(&assertive_handle_id));
        assert_eq!(res.not_executed.len(), 1);
        let not_executed = res.not_executed.pop_first().unwrap();
        let reply_msg_id = MessageId::generate_reply(not_executed);
        assert!(res.succeed.contains(&reply_msg_id));
    }
}
