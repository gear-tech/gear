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

#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::collections::BTreeSet;
use codec::{Decode, Encode};
use gstd::{debug, exec, msg, prelude::*, ActorId, MessageId};

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
    Add(u64),
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Yes,
    No,
    NotNeeded,
    Success,
    Failure,
}

enum TransitionState {
    Ready,
    NotReady,
    Failed,
}

struct Transition {
    #[allow(dead_code)]
    to_status: u32,
    origin: ActorId,
    query_list: Vec<ActorId>,
    message_id: MessageId,
    last_sent_message_id: MessageId,
    query_index: usize,
    state: TransitionState,
}

struct NodeState {
    #[allow(dead_code)]
    status: u32,
    sub_nodes: BTreeSet<ActorId>,
    transition: Option<Transition>,
}

static mut STATE: Option<NodeState> = None;

#[no_mangle]
extern "C" fn handle() {
    let reply = match msg::load() {
        Ok(request) => process(request),
        Err(e) => {
            debug!("Error processing request: {:?}", e);
            Reply::Failure
        }
    };

    msg::reply(reply, 0).unwrap();
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
                match transition.state {
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

            transition.last_sent_message_id = msg::send(*next_sub_node, request, 0).unwrap();

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

                if !state().sub_nodes.is_empty() {
                    debug!("Transition started is complex");

                    transition.query_list = state().sub_nodes.iter().cloned().collect();
                    let first_sub_node = *transition
                        .query_list
                        .get(0)
                        .expect("Checked above that sub_nodes is not empty; qed");
                    transition.last_sent_message_id =
                        msg::send(first_sub_node, request, 0).unwrap();
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
            if state().sub_nodes.is_empty() {
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
            } else if let Some(mut transition) = state().transition.take() {
                if let TransitionState::Ready = transition.state {
                    let first_sub_node = *transition
                        .query_list
                        .get(0)
                        .expect("Checked above that sub_nodes is not empty; qed");

                    transition.query_index = 0;

                    transition.message_id = msg::id();

                    transition.last_sent_message_id =
                        msg::send(first_sub_node, request, 0).unwrap();

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
        Request::Add(sub_node) => {
            state().sub_nodes.insert(sub_node.into());
            Reply::Success
        }
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    if let Some(ref mut transition) = state().transition {
        if msg::reply_to().unwrap() != transition.last_sent_message_id {
            return;
        }

        match msg::load() {
            Ok(reply) => {
                transition.query_index += 1;
                if let Reply::Success = reply {
                } else {
                    transition.state = TransitionState::Failed;
                }
                exec::wake(transition.message_id).unwrap();
            }
            Err(e) => {
                transition.state = TransitionState::Failed;
                debug!("Error processing reply: {:?}", e);
                exec::wake(transition.message_id).unwrap();
            }
        }
    } else {
        debug!("Got some reply that can not be processed");
    }
}

#[no_mangle]
extern "C" fn init() {
    let init: Initialization = msg::load().expect("Failed to decode init");

    unsafe {
        STATE = Some(NodeState {
            status: init.status,
            sub_nodes: BTreeSet::default(),
            transition: None,
        });
    }

    msg::reply((), 0).unwrap();
}

#[cfg(test)]
mod tests {
    use super::{Initialization, Operation, Reply, Request};
    use gtest::{Log, Program, System};

    #[test]
    fn test_message_send_to_failed_program() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let res = program.send(from, Request::IsReady);
        assert!(res.main_failed());
    }

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let res = program.send(from, Initialization { status: 5 });
        let log = Log::builder().source(program.id()).dest(from);
        assert!(!res.main_failed());
        assert!(res.contains(&log));
    }

    #[test]
    fn one_node_can_change_status() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let _res = program.send(from, Initialization { status: 5 });

        let res = program.send(from, Request::IsReady);
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Yes);
        assert!(res.contains(&log));

        let res = program.send(from, Request::Begin(Operation { to_status: 7 }));
        let log = Log::builder()
            .source(program.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program.send(from, Request::Commit);
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

        let from = 42;

        let program_1_id = 1;
        let program_2_id = 2;
        let program_3_id = 3;

        let program_1 = Program::current_with_id(&system, program_1_id);
        let _res = program_1.send(from, Initialization { status: 5 });

        let program_2 = Program::current_with_id(&system, program_2_id);
        let _res = program_2.send(from, Initialization { status: 5 });

        let program_3 = Program::current_with_id(&system, program_3_id);
        let _res = program_3.send(from, Initialization { status: 9 });

        let res = program_1.send(from, Request::Add(program_2_id));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Add(program_3_id));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Begin(Operation { to_status: 7 }));
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));

        let res = program_1.send(from, Request::Commit);
        let log = Log::builder()
            .source(program_1.id())
            .dest(from)
            .payload(Reply::Success);
        assert!(res.contains(&log));
    }
}
