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

#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub struct Operation {
    to_status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub struct Initialization {
    status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    IsReady,
    Begin(Operation),
    Commit,
    Add(u64),
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Yes,
    No,
    NotNeeded,
    Success,
    Failure,
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::BTreeSet;
    use gstd::{debug, exec, msg, prelude::*, ActorId, MessageId};

    use super::{Initialization, Operation, Reply, Request};

    #[derive(Clone)]
    enum TransitionState {
        Ready,
        NotReady,
        Commited,
        Failed,
    }

    struct Transition {
        to_status: u32,
        origin: ActorId,
        query_list: Vec<ActorId>,
        message_id: MessageId,
        last_sent_message_id: MessageId,
        query_index: usize,
        state: TransitionState,
    }

    struct NodeState {
        status: u32,
        sub_nodes: BTreeSet<ActorId>,
        transition: Option<Transition>,
    }

    static mut STATE: Option<NodeState> = None;

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let reply = match msg::load() {
            Ok(request) => process(request),
            Err(e) => {
                debug!("Error processing request: {:?}", e);
                Reply::Failure
            }
        };

        msg::reply(reply, exec::gas_available() - 20_500_000, 0);
    }

    fn state() -> &'static mut NodeState {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: Request) -> Reply {
        if let Some(mut transition) = state().transition.take() {
            if transition.message_id == msg::id() {
                // one of the answers has set failed state
                if let TransitionState::Failed = transition.state {
                    return Reply::Failure;
                }

                // this means that we received replies from all subnodes
                if transition.query_index == transition.query_list.len() {
                    match transition.state.clone() {
                        TransitionState::NotReady => {
                            transition.state = TransitionState::Ready;

                            debug!("Returning final ready signal");

                            // this is ready to further process with committing
                            state().transition = Some(transition);
                            return Reply::Success;
                        }
                        TransitionState::Ready => {
                            // this means we successfully commited and we can
                            // drop the transition returning success
                            debug!("Returning final commit signal");

                            return Reply::Success;
                        }
                        _ => {
                            // this is some invalid state already
                            return Reply::Failure;
                        }
                    }
                }

                // this means we need to send another sub-node query
                let next_sub_node = transition
                    .query_list
                    .get(transition.query_index)
                    .expect("Checked above that it has that number of elements; qed");

                transition.last_sent_message_id = msg::send(
                    *next_sub_node,
                    request,
                    exec::gas_available() - 100_000_000,
                    0,
                );

                state().transition = Some(transition);

                exec::wait();
            } else {
                // this is just a new message that should be processed normally, without continuation.
                state().transition = Some(transition);
            }
        }

        match request {
            Request::IsReady => {
                if state().transition.is_none() {
                    Reply::Yes
                } else {
                    Reply::No
                }
            }
            Request::Begin(Operation { to_status }) => {
                if state().transition.is_some() {
                    Reply::Failure
                } else {
                    let mut transition = Transition {
                        to_status,
                        origin: msg::source(),
                        query_index: 0,
                        query_list: vec![],
                        state: TransitionState::NotReady,
                        message_id: msg::id(),
                        last_sent_message_id: MessageId::default(),
                    };

                    debug!("Transition started");

                    if state().sub_nodes.len() > 0 {
                        debug!("Transition started is complex");

                        transition.query_list = state().sub_nodes.iter().cloned().collect();
                        let first_sub_node = *transition
                            .query_list
                            .get(0)
                            .expect("Checked above that sub_nodes is not empty; qed");
                        transition.last_sent_message_id = msg::send(
                            first_sub_node,
                            request,
                            exec::gas_available() - 100_000_000,
                            0,
                        );
                        state().transition = Some(transition);
                        exec::wait()
                    } else {
                        transition.state = TransitionState::Ready;
                        state().transition = Some(transition);
                        Reply::Success
                    }
                }
            }
            Request::Commit => {
                if state().sub_nodes.len() == 0 {
                    let (transition, reply) = match state().transition.take() {
                        Some(transition) => {
                            if transition.origin != msg::source() {
                                (Some(transition), Reply::Failure)
                            } else {
                                (None, Reply::Success)
                            }
                        }
                        None => (None, Reply::Failure),
                    };

                    state().transition = transition;

                    reply
                } else {
                    if let Some(mut transition) = state().transition.take() {
                        if let TransitionState::Ready = transition.state {
                            let first_sub_node = *transition
                                .query_list
                                .get(0)
                                .expect("Checked above that sub_nodes is not empty; qed");

                            transition.query_index = 0;

                            transition.message_id = msg::id();

                            transition.last_sent_message_id = msg::send(
                                first_sub_node,
                                request,
                                exec::gas_available() - 100_000_000,
                                0,
                            );

                            state().transition = Some(transition);

                            exec::wait()
                        } else {
                            debug!("Returning failure because current state is not READY");
                            Reply::Failure
                        }
                    } else {
                        debug!("Returning failure because there is no transition in process");
                        Reply::Failure
                    }
                }
            }
            Request::Add(sub_node) => {
                state().sub_nodes.insert((sub_node as u64).into());
                Reply::Success
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {
        if let Some(ref mut transition) = state().transition {
            if msg::reply_to() != transition.last_sent_message_id {
                return;
            }

            match msg::load() {
                Ok(reply) => {
                    transition.query_index += 1;
                    if let Reply::Success = reply {
                    } else {
                        transition.state = TransitionState::Failed;
                    }
                    exec::wake(transition.message_id.into());
                }
                Err(e) => {
                    transition.state = TransitionState::Failed;
                    debug!("Error processing reply: {:?}", e);
                    exec::wake(transition.message_id.into());
                }
            }
        } else {
            debug!("Got some reply that can not be processed");
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let init: Initialization = msg::load().expect("Failed to decode init");

        STATE = Some(NodeState {
            status: init.status,
            sub_nodes: BTreeSet::default(),
            transition: None,
        });

        msg::reply((), exec::gas_available() - 20_500_000, 0);
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::{native, Initialization, Operation, Reply, Request};
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

        // Assertions are performed when decoding reply
        let _reply: () = runner.init_program_with_reply(
            InitProgram::from(wasm_code()).message(Initialization { status: 5 }),
        );
    }

    #[test]
    fn one_node_can_change_status() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default()).try_init();

        let mut runner = RunnerContext::default();

        runner.init_program(InitProgram::from(wasm_code()).message(Initialization { status: 5 }));

        let reply: Reply = runner.request(Request::IsReady);
        assert_eq!(reply, Reply::Yes);

        let reply: Reply = runner.request(Request::Begin(Operation { to_status: 7 }));
        assert_eq!(reply, Reply::Success);

        let reply: Reply = runner.request(Request::Commit);
        assert_eq!(reply, Reply::Success);
    }

    #[test]
    fn multiple_nodes_can_prepare_to_change_status() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default()).try_init();

        let mut runner = RunnerContext::default();

        let program_id_1 = 1;
        let program_id_2 = 2;
        let program_id_3 = 3;

        runner.init_program(
            InitProgram::from(wasm_code())
                .message(Initialization { status: 5 })
                .id(program_id_1),
        );
        runner.init_program(
            InitProgram::from(wasm_code())
                .message(Initialization { status: 5 })
                .id(program_id_2),
        );
        runner.init_program(
            InitProgram::from(wasm_code())
                .message(Initialization { status: 9 })
                .id(program_id_3),
        );

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Add(2)).destination(program_id_1));
        assert_eq!(reply, Reply::Success);

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Add(3)).destination(program_id_1));
        assert_eq!(reply, Reply::Success);

        let reply: Reply = runner.request(
            MessageBuilder::from(Request::Begin(Operation { to_status: 7 }))
                .destination(program_id_1),
        );
        assert_eq!(reply, Reply::Success);

        let reply: Reply =
            runner.request(MessageBuilder::from(Request::Commit).destination(program_id_1));
        assert_eq!(reply, Reply::Success);
    }
}
