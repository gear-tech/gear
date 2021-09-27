// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::prelude::*;

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    Receive(u64),
    Join(u64),
    Report,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Success,
    Failure,
    StateFailure,
    Amount(u64),
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::BTreeSet;
    use alloc::rc::Rc;
    use codec::{Decode, Encode};
    use core::cell::RefCell;
    use core::future::Future;
    use gstd::{exec, ext, msg, prelude::*, ProgramId};

    use super::{Reply, Request};

    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct Program {
        handle: ProgramId,
    }

    struct ProgramState {
        nodes: Rc<RefCell<BTreeSet<Program>>>,
        amount: u64,
    }

    impl Default for ProgramState {
        fn default() -> Self {
            Self {
                nodes: Rc::new(RefCell::new(BTreeSet::default())),
                amount: 0,
            }
        }
    }

    static mut STATE: Option<ProgramState> = None;

    impl Program {
        fn new(handle: impl Into<ProgramId>) -> Self {
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
                let reply_bytes = gstd_async::msg::send_and_wait_for_reply(
                    program_handle,
                    &encoded_request[..],
                    exec::gas_available() - 25_000_000,
                    0,
                )
                .await;

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

        fn nodes() -> &'static Rc<RefCell<BTreeSet<Program>>> {
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
                    ext::debug(&format!("Error processing request: {:?}", e));
                    Reply::Failure
                }
            };

            ext::debug("Handle request finished");
            msg::reply(reply, exec::gas_available() - 25_000_000, 0);
        }

        async fn handle_receive(amount: u64) -> Reply {
            ext::debug(&format!("Handling receive {}", amount));

            let (subnodes_count, nodes) = match Program::nodes().try_borrow() {
                Ok(nodes) => (nodes.len() as u64, nodes),
                Err(_) => {
                    return Reply::StateFailure;
                }
            };

            if subnodes_count > 0 {
                let distributed_per_node = amount / subnodes_count;
                let distributed_total = distributed_per_node * subnodes_count;
                let mut left_over = amount - distributed_total;

                if distributed_per_node > 0 {
                    for program in nodes.iter() {
                        if let Err(_) = program.do_send(distributed_per_node).await {
                            // reclaiming amount from nodes that fail!
                            left_over += distributed_per_node;
                        }
                    }
                }

                ext::debug(&format!("Set own amount to: {}", left_over));
                *Self::amount() = *Self::amount() + left_over;
            } else {
                ext::debug(&format!("Set own amount to: {}", amount));
                *Self::amount() = *Self::amount() + amount;
            }

            Reply::Success
        }

        async fn handle_join(program_id: u64) -> Reply {
            let mut nodes = match Self::nodes().try_borrow_mut() {
                Ok(nodes) => nodes,
                Err(_) => {
                    return Reply::StateFailure; // Probably receive in progress, so nodes cannot be altered!
                }
            };

            nodes.insert(Program::new(program_id));

            Reply::Success
        }

        async fn handle_report() -> Reply {
            let mut amount = *Program::amount();
            ext::debug(&format!("Own amount: {}", amount));

            let nodes = match Program::nodes().try_borrow() {
                Ok(nodes) => nodes,
                Err(_) => {
                    return Reply::StateFailure;
                }
            };

            for program in nodes.iter() {
                ext::debug("Querying next node");
                amount += match program.do_report().await {
                    Ok(amount) => {
                        ext::debug(&format!("Sub-node result: {}", amount));
                        amount
                    }
                    Err(_) => {
                        // skipping erroneous sub-nodes!
                        ext::debug("Skipping errorneous node");
                        0
                    }
                }
            }

            Reply::Amount(amount)
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        ext::debug("Handling sequence started");
        gstd_async::block_on(Program::handle_request());
        ext::debug("Handling sequence terminated");
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        STATE = Some(ProgramState::default());
        ext::debug("Program initialized");
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::{native, Reply, Request};

    use common::*;

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY_BLOATY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = RunnerContext::default();

        runner.init_program(InitProgram::from(wasm_code()).message(b"init"));

        let _ = runner.storage();
    }

    #[test]
    fn single_program() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let reply: Reply = runner.request(Request::Receive(10));
        assert_eq!(reply, Reply::Success);

        let reply: Reply = runner.request(Request::Report);
        assert_eq!(reply, Reply::Amount(10));
    }

    fn multi_program_setup(
        program_id_1: u64,
        program_id_2: u64,
        program_id_3: u64,
    ) -> RunnerContext {
        let mut runner = RunnerContext::default();

        runner.init_program(InitProgram::from(wasm_code()).id(program_id_1));
        runner.init_program(InitProgram::from(wasm_code()).id(program_id_2));
        runner.init_program(InitProgram::from(wasm_code()).id(program_id_3));

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Join(2)).destination(program_id_1));
        assert_eq!(reply, Reply::Success);

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Join(3)).destination(program_id_1));
        assert_eq!(reply, Reply::Success);

        runner
    }

    #[test]
    fn composite_program() {
        env_logger::Builder::from_env(env_logger::Env::default()).init();

        let program_id_1 = 1;
        let program_id_2 = 2;
        let program_id_3 = 3;

        let mut runner = multi_program_setup(program_id_1, program_id_2, program_id_3);

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Receive(11)).destination(program_id_1));
        assert_eq!(reply, Reply::Success);

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Report).destination(program_id_2));
        assert_eq!(reply, Reply::Amount(5));

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Report).destination(program_id_1));
        assert_eq!(reply, Reply::Amount(11));
    }

    // This test show how RefCell will prevent to do conficting changes (prevent multi-aliasing of the program state)
    #[test]
    fn conflicting_nodes() {
        let program_id_1 = 1;
        let program_id_2 = 2;
        let program_id_3 = 3;
        let program_id_4 = 4;

        let mut runner = multi_program_setup(program_id_1, program_id_2, program_id_3);
        runner.init_program(InitProgram::from(wasm_code()).id(program_id_4));

        let results: Vec<Reply> = runner.request_batch(vec![
            MessageBuilder::from(Request::Receive(11)).destination(program_id_1),
            MessageBuilder::from(Request::Join(4)).destination(program_id_1),
        ]);

        assert_eq!(results, vec![Reply::Success, Reply::StateFailure])
    }
}
