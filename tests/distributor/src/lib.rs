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
    use gstd::{ext, msg, prelude::*, ProgramId};

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
                    msg::gas_available() - 25_000_000,
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
            msg::reply(reply, msg::gas_available() - 25_000_000, 0);
        }

        async fn handle_receive(amount: u64) -> Reply {
            ext::debug(&format!("Handling receive {}", amount));
            let subnodes_count = Self::nodes().borrow().len() as u64;
            if subnodes_count > 0 {
                let distributed_per_node = amount / subnodes_count;
                let distributed_total = distributed_per_node * subnodes_count;
                let mut left_over = amount - distributed_total;

                if distributed_per_node > 0 {
                    for program in Program::nodes().borrow().iter() {
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
            Self::nodes().borrow_mut().insert(Program::new(program_id));

            Reply::Success
        }

        async fn handle_report() -> Reply {
            let mut amount = *Program::amount();
            ext::debug(&format!("Own amount: {}", amount));

            for program in Program::nodes().borrow().iter() {
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

    use gear_core::{
        program::ProgramId,
        storage::{InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList},
    };
    use gear_core_runner::{Config, ExtMessage, InitializeProgramInfo, Runner};

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    pub type LocalRunner = Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList>;

    fn new_test_runner() -> LocalRunner {
        Runner::new(&Config::default(), Default::default())
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = new_test_runner();

        runner
            .init_program(InitializeProgramInfo {
                new_program_id: 1.into(),
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        let _ = runner.complete();
    }

    #[test]
    fn single_program() {
        let runner = new_test_runner();

        let program_id_1: ProgramId = 1.into();

        let mut nonce = 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: 1.into(),
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: (),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Receive(10),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (_runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Report,
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Amount(10)));
    }

    #[test]
    fn composite_program() {
        env_logger::Builder::from_env(env_logger::Env::default()).init();

        let runner = new_test_runner();

        let program_id_1: ProgramId = 1.into();
        let program_id_2: ProgramId = 2.into();
        let program_id_3: ProgramId = 3.into();

        let mut nonce = 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_1,
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: (),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        nonce += 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_2,
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: (),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        nonce += 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_3,
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: (),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Join(2),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Join(3),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Receive(11),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_2,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Report,
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Amount(5)));

        let (_runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Report,
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Amount(11)));
    }
}
