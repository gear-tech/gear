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
use gstd::{prelude::*, ActorId};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    Receive(u64),
    Join(u64),
    Report,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Success,
    Failure,
    StateFailure,
    Amount(u64),
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Program {
    handle: ActorId,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;

    use alloc::collections::BTreeSet;
    use core::future::Future;
    use gstd::{debug, lock::Mutex, msg};

    static mut STATE: Option<ProgramState> = None;

    struct ProgramState {
        nodes: Mutex<BTreeSet<Program>>,
        amount: u64,
    }

    impl Default for ProgramState {
        fn default() -> Self {
            Self {
                nodes: Mutex::new(BTreeSet::default()),
                amount: 0,
            }
        }
    }

    impl Program {
        fn new(handle: impl Into<ActorId>) -> Self {
            Self {
                handle: handle.into(),
            }
        }

        fn do_request<Req: Encode, Rep: Decode>(
            &self,
            request: Req,
        ) -> impl Future<Output = Result<Rep, &'static str>> {
            let encoded_request: Vec<u8> = request.encode();

            let program_handle = self.handle;
            async move {
                let reply_bytes =
                    msg::send_bytes_for_reply(program_handle, &encoded_request[..], 0)
                        .expect("Error in message sending")
                        .await
                        .expect("Error in async message processing");

                Rep::decode(&mut &reply_bytes[..]).map_err(|_| "Failed to decode reply")
            }
        }

        async fn do_send(&self, amount: u64) -> Result<(), &'static str> {
            match self.do_request(Request::Receive(amount)).await? {
                Reply::Success => Ok(()),
                _ => Err("Unexpected send reply"),
            }
        }

        async fn do_report(&self) -> Result<u64, &'static str> {
            match self.do_request(Request::Report).await? {
                Reply::Amount(amount) => Ok(amount),
                _ => Err("Unexpected send reply"),
            }
        }

        fn nodes() -> &'static Mutex<BTreeSet<Program>> {
            unsafe { &mut STATE.as_mut().expect("STATE UNITIALIZED!").nodes }
        }

        fn amount() -> &'static mut u64 {
            unsafe { &mut STATE.as_mut().expect("STATE UNITIALIZED!").amount }
        }

        async fn handle_request() {
            let reply = match msg::load::<Request>() {
                Ok(request) => match request {
                    Request::Receive(amount) => Self::handle_receive(amount).await,
                    Request::Join(program_id) => Self::handle_join(program_id).await,
                    Request::Report => Self::handle_report().await,
                },
                Err(e) => {
                    debug!("Error processing request: {:?}", e);
                    Reply::Failure
                }
            };

            debug!("Handle request finished");
            msg::reply(reply, 0).unwrap();
        }

        async fn handle_receive(amount: u64) -> Reply {
            debug!("Handling receive {}", amount);

            let nodes = Program::nodes().lock().await;
            let subnodes_count = nodes.as_ref().len() as u64;

            if subnodes_count > 0 {
                let distributed_per_node = amount / subnodes_count;
                let distributed_total = distributed_per_node * subnodes_count;
                let mut left_over = amount - distributed_total;

                if distributed_per_node > 0 {
                    for program in nodes.as_ref().iter() {
                        if program.do_send(distributed_per_node).await.is_err() {
                            // reclaiming amount from nodes that fail!
                            left_over += distributed_per_node;
                        }
                    }
                }

                debug!("Set own amount to: {}", left_over);
                *Self::amount() = *Self::amount() + left_over;
            } else {
                debug!("Set own amount to: {}", amount);
                *Self::amount() = *Self::amount() + amount;
            }

            Reply::Success
        }

        async fn handle_join(program_id: u64) -> Reply {
            let mut nodes = Self::nodes().lock().await;
            debug!("Inserting into nodes");
            nodes.as_mut().insert(Program::new(program_id));
            Reply::Success
        }

        async fn handle_report() -> Reply {
            let mut amount = *Program::amount();
            debug!("Own amount: {}", amount);

            let nodes = Program::nodes().lock().await;

            for program in nodes.as_ref().iter() {
                debug!("Querying next node");
                amount += match program.do_report().await {
                    Ok(amount) => {
                        debug!("Sub-node result: {}", amount);
                        amount
                    }
                    Err(_) => {
                        // skipping erroneous sub-nodes!
                        debug!("Skipping errorneous node");
                        0
                    }
                }
            }

            Reply::Amount(amount)
        }
    }

    #[no_mangle]
    extern "C" fn handle() {
        debug!("Handling sequence started");
        gstd::message_loop(Program::handle_request());
        debug!("Handling sequence terminated");
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        gstd::record_reply();
    }

    #[no_mangle]
    extern "C" fn init() {
        unsafe { STATE = Some(Default::default()) };
        msg::reply((), 0).unwrap();
        debug!("Program initialized");
    }
}

#[cfg(test)]
mod tests {
    use super::{Reply, Request};
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
    fn single_program() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let _res = program.send_bytes(from, b"init");

        let res = program.send(from, Request::Receive(10));
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program.send(from, Request::Report);
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
    ) -> (Program, Program, Program) {
        system.init_logger();

        let from = 42;

        let program_1 = Program::current_with_id(system, program_1_id);
        let _res = program_1.send_bytes(from, b"init");

        let program_2 = Program::current_with_id(system, program_2_id);
        let _res = program_2.send_bytes(from, b"init");

        let program_3 = Program::current_with_id(system, program_3_id);
        let _res = program_3.send_bytes(from, b"init");

        let res = program_1.send(from, Request::Join(program_2_id));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Join(program_3_id));
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

        let from = 42;

        let res = program_1.send(from, Request::Receive(11));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_2.send(from, Request::Report);
        let log = Log::builder()
            .source(program_2.id())
            .dest(from)
            .payload(Reply::Amount(5));
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Report);
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
        let from = 42;

        let program_4 = Program::current_with_id(&system, program_4_id);
        let _res = program_4.send_bytes(from, b"init");

        IntoIterator::into_iter([Request::Receive(11), Request::Join(program_4_id)])
            .map(|request| program_1.send(from, request))
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
