// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use gstd::msg;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    gstd::debug!("Received payload: {payload:?}");
    if payload == b"PING" {
        msg::reply_bytes("INIT_PONG", 0).expect("Failed to send reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"PING" {
        msg::reply_bytes("HANDLE_PONG", 0).expect("Failed to send reply");
    }
}

#[cfg(test)]
mod tests {
    use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn test_init() {
        gtest::ensure_gbuild(false);

        // Initialize system environment
        let system = System::new();
        system.init_logger();

        // Get program from artifact
        let user = DEFAULT_USER_ALICE;
        let program = Program::current(&system);

        // Init program
        let msg_id = program.send_bytes(user, b"PING");
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
        assert!(res.contains(&(user, b"INIT_PONG")));

        // Handle program
        let msg_id = program.send_bytes(user, b"PING");
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
        assert!(res.contains(&(user, b"HANDLE_PONG")));
    }
}
