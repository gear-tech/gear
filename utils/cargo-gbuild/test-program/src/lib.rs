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
