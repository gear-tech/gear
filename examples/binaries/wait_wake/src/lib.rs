// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use codec::{Decode, Encode};
use gstd::{exec, msg, prelude::*, MessageId};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    EchoWait(u32),
    Wake(MessageId),
}

static mut ECHOES: Option<BTreeMap<MessageId, u32>> = None;

fn process_request(request: Request) {
    match request {
        Request::EchoWait(n) => {
            unsafe {
                ECHOES
                    .get_or_insert_with(BTreeMap::new)
                    .insert(msg::id(), n)
            };
            exec::wait();
        }
        Request::Wake(id) => exec::wake(id).unwrap(),
    }
}

#[no_mangle]
extern "C" fn init() {
    msg::reply((), 0).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    if let Some(reply) = unsafe { ECHOES.get_or_insert_with(BTreeMap::new).remove(&msg::id()) } {
        msg::reply(reply, 0).unwrap();
    } else {
        msg::load::<Request>().map(process_request).unwrap();
    }
}

#[no_mangle]
extern "C" fn handle_reply() {}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::Request;
    use std::convert::TryInto;

    use gstd::MessageId;
    use gtest::{Log, Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let res = program.send_bytes(from, b"init");
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn wake_self() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let _res = program.send_bytes(from, b"init");

        let msg_1_echo_wait = 100;
        let res = program.send(from, Request::EchoWait(msg_1_echo_wait));
        let msg_id_1 = res.sent_message_id();
        assert!(res.log().is_empty());

        let msg_2_echo_wait = 200;
        let res = program.send(from, Request::EchoWait(msg_2_echo_wait));
        let msg_id_2 = res.sent_message_id();
        assert!(res.log().is_empty());

        let res = program.send(
            from,
            Request::Wake(MessageId::new(msg_id_1.as_ref().try_into().unwrap())),
        );
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(msg_1_echo_wait);
        assert!(res.contains(&log));

        let res = program.send(
            from,
            Request::Wake(MessageId::new(msg_id_2.as_ref().try_into().unwrap())),
        );
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

        let from = 42;

        let program_1 = Program::current(&system);
        let _res = program_1.send_bytes(from, b"init");

        let program_2 = Program::current(&system);
        let _res = program_2.send_bytes(from, b"init");

        let msg_1_echo_wait = 100;
        let res = program_1.send(from, Request::EchoWait(msg_1_echo_wait));
        let msg_id_1 = res.sent_message_id();
        assert!(res.log().is_empty());

        let msg_2_echo_wait = 200;
        let res = program_2.send(from, Request::EchoWait(msg_2_echo_wait));
        let msg_id_2 = res.sent_message_id();
        assert!(res.log().is_empty());

        // try to wake other messages
        let res = program_2.send(
            from,
            Request::Wake(MessageId::new(msg_id_1.as_ref().try_into().unwrap())),
        );
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload_bytes([]);
        assert!(res.contains(&log));

        let res = program_1.send(
            from,
            Request::Wake(MessageId::new(msg_id_2.as_ref().try_into().unwrap())),
        );
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload_bytes([]);
        assert!(res.contains(&log));

        // wake msg_1 for program_1 and msg_2 for program_2
        let res = program_1.send(
            from,
            Request::Wake(MessageId::new(msg_id_1.as_ref().try_into().unwrap())),
        );
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(msg_1_echo_wait);
        assert!(res.contains(&log));

        let res = program_2.send(
            from,
            Request::Wake(MessageId::new(msg_id_2.as_ref().try_into().unwrap())),
        );
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload(msg_2_echo_wait);
        assert!(res.contains(&log));
    }
}
