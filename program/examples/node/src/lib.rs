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

extern crate alloc;

use gstd::ActorId;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub struct Operation {
    to_status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub struct Initialization {
    pub status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    IsReady,
    Begin(Operation),
    Commit,
    Add(ActorId),
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Yes,
    No,
    NotNeeded,
    Success,
    Failure,
}

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use super::{Initialization, Operation, Reply, Request};
    use gtest::{Log, Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn test_message_send_to_failed_program() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program = Program::current(&system);
        let msg_id = program.send(from, Request::IsReady);
        let res = system.run_next_block();
        assert!(res.failed.contains(&msg_id));
    }

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program = Program::current(&system);
        let msg_id = program.send(from, Initialization { status: 5 });
        let log = Log::builder().source(program.id()).dest(from);
        let res = system.run_next_block();
        assert!(res.succeed.contains(&msg_id));
        assert!(res.contains(&log));
    }

    #[test]
    fn one_node_can_change_status() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program = Program::current(&system);
        program.send(from, Initialization { status: 5 });

        program.send(from, Request::IsReady);
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Yes);
        let res = system.run_next_block();
        assert!(res.contains(&log));

        program.send(from, Request::Begin(Operation { to_status: 7 }));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program.send(from, Request::Commit);
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));
    }

    #[test]
    fn multiple_nodes_can_prepare_to_change_status() {
        let system = System::new();
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program_1_id = 1;
        let program_2_id = 2;
        let program_3_id = 3;

        let program_1 = Program::current_with_id(&system, program_1_id);
        program_1.send(from, Initialization { status: 5 });

        let program_2 = Program::current_with_id(&system, program_2_id);
        program_2.send(from, Initialization { status: 5 });

        let program_3 = Program::current_with_id(&system, program_3_id);
        program_3.send(from, Initialization { status: 9 });

        program_1.send(from, Request::Add(program_2_id.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program_1.send(from, Request::Add(program_3_id.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program_1.send(from, Request::Begin(Operation { to_status: 7 }));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program_1.send(from, Request::Commit);
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));
    }
}
