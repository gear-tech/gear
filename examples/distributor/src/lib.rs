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

use gstd::{prelude::*, ActorId};
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    Receive(u64),
    Join(ActorId),
    Report,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Success,
    Failure,
    StateFailure,
    Amount(u64),
}

#[allow(dead_code)]
#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Program {
    handle: ActorId,
}

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    use super::{Reply, Request};
    use gtest::{constants::DEFAULT_USER_ALICE, Log, Program, System};

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
    fn single_program() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = DEFAULT_USER_ALICE;

        // Init
        program.send_bytes(from, b"init");

        program.send(from, Request::Receive(10));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program.send(from, Request::Report);
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Amount(10));
        assert!(res.contains(&log));
    }

    fn multi_program_setup(
        system: &System,
        program_1_id: u64,
        program_2_id: u64,
        program_3_id: u64,
    ) -> (Program<'_>, Program<'_>, Program<'_>) {
        system.init_logger();

        let from = DEFAULT_USER_ALICE;

        let program_1 = Program::current_with_id(system, program_1_id);
        program_1.send_bytes(from, b"init");

        let program_2 = Program::current_with_id(system, program_2_id);
        program_2.send_bytes(from, b"init");

        let program_3 = Program::current_with_id(system, program_3_id);
        program_3.send_bytes(from, b"init");

        program_1.send(from, Request::Join(program_2_id.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program_1.send(from, Request::Join(program_3_id.into()));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        (program_1, program_2, program_3)
    }

    #[test]
    fn composite_program() {
        let system = System::new();
        let (program_1, program_2, _program_3) = multi_program_setup(&system, 1, 2, 3);

        let from = DEFAULT_USER_ALICE;

        program_1.send(from, Request::Receive(11));
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        program_2.send(from, Request::Report);
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload(Reply::Amount(5));
        assert!(res.contains(&log));

        program_1.send(from, Request::Report);
        let res = system.run_next_block();
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Amount(11));
        assert!(res.contains(&log));
    }

    // This test show how RefCell will prevent to do conflicting changes (prevent multi-aliasing of the program state)
    #[test]
    fn conflicting_nodes() {
        let system = System::new();
        let (program_1, _program_2, _program_3) = multi_program_setup(&system, 1, 2, 3);

        let program_4_id = 4;
        let from = DEFAULT_USER_ALICE;

        let program_4 = Program::current_with_id(&system, program_4_id);
        program_4.send_bytes(from, b"init");

        IntoIterator::into_iter([Request::Receive(11), Request::Join(program_4_id.into())])
            .map(|request| {
                program_1.send(from, request);
                system.run_next_block()
            })
            .zip(IntoIterator::into_iter([Reply::Success, Reply::Success]))
            .for_each(|(result, reply)| {
                let log = Log::builder()
                    .source(program_1.id())
                    .dest(from)
                    .payload(reply);
                assert!(result.contains(&log));
            });
    }
}
