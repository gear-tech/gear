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

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    EchoWait(u32),
    Wake([u8; 32]),
}

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    extern crate std;

    use super::Request;
    use gtest::{Log, Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = DEFAULT_USER_ALICE;

        program.send_bytes(from, b"init");
        let res = system.run_next_block();
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn wake_self() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program = Program::current(&system);
        program.send_bytes(from, b"init");
        system.run_next_block();

        let msg_1_echo_wait = 100;
        let msg_id_1 = program.send(from, Request::EchoWait(msg_1_echo_wait));
        let res = system.run_next_block();
        assert!(res.log().is_empty());

        let msg_2_echo_wait = 200;
        let msg_id_2 = program.send(from, Request::EchoWait(msg_2_echo_wait));
        let res = system.run_next_block();
        assert!(res.log().is_empty());

        program.send(from, Request::Wake(msg_id_1.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(msg_1_echo_wait);
        assert!(res.contains(&log));

        program.send(from, Request::Wake(msg_id_2.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(msg_2_echo_wait);
        assert!(res.contains(&log));
    }

    #[test]
    fn wake_other() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program_1 = Program::current(&system);
        program_1.send_bytes(from, b"init");
        system.run_next_block();

        let program_2 = Program::current(&system);
        program_2.send_bytes(from, b"init");
        system.run_next_block();

        let msg_1_echo_wait = 100;
        let msg_id_1 = program_1.send(from, Request::EchoWait(msg_1_echo_wait));
        let res = system.run_next_block();
        assert!(res.log().is_empty());

        let msg_2_echo_wait = 200;
        let msg_id_2 = program_2.send(from, Request::EchoWait(msg_2_echo_wait));
        let res = system.run_next_block();
        assert!(res.log().is_empty());

        // try to wake other messages
        program_2.send(from, Request::Wake(msg_id_1.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload_bytes([]);
        assert!(res.contains(&log));

        program_1.send(from, Request::Wake(msg_id_2.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload_bytes([]);
        assert!(res.contains(&log));

        // wake msg_1 for program_1 and msg_2 for program_2
        program_1.send(from, Request::Wake(msg_id_1.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(msg_1_echo_wait);
        assert!(res.contains(&log));

        program_2.send(from, Request::Wake(msg_id_2.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload(msg_2_echo_wait);
        assert!(res.contains(&log));
    }
}
