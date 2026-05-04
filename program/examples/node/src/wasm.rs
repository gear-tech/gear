// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::{Initialization, Operation, Reply, Request};
use gstd::{ActorId, MessageId, collections::BTreeSet, debug, exec, msg, prelude::*};

static mut STATE: Option<NodeState> = None;

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

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let reply = match msg::load() {
        Ok(request) => process(request),
        Err(e) => {
            debug!("Error processing request: {e:?}");
            Reply::Failure
        }
    };

    msg::reply(reply, 0).unwrap();
}

fn state() -> &'static mut NodeState {
    unsafe { static_mut!(STATE).as_mut().unwrap() }
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
                        // this means we successfully committed and we can
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
                        .first()
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
                        .first()
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
            state().sub_nodes.insert(sub_node);
            Reply::Success
        }
    }
}

#[unsafe(no_mangle)]
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
                debug!("Error processing reply: {e:?}");
                exec::wake(transition.message_id).unwrap();
            }
        }
    } else {
        debug!("Got some reply that can not be processed");
    }
}

#[unsafe(no_mangle)]
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
