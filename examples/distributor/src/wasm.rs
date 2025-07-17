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

use crate::{Program, Reply, Request};
use core::future::Future;
use gstd::{ActorId, collections::BTreeSet, debug, msg, prelude::*, sync::Mutex};
use parity_scale_codec::{Decode, Encode};

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
            let reply_bytes = msg::send_bytes_for_reply(program_handle, &encoded_request[..], 0, 0)
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
        unsafe {
            &static_mut!(STATE)
                .as_mut()
                .expect("STATE UNINITIALIZED!")
                .nodes
        }
    }

    fn amount() -> &'static mut u64 {
        unsafe {
            &mut static_mut!(STATE)
                .as_mut()
                .expect("STATE UNINITIALIZED!")
                .amount
        }
    }

    async fn handle_request() {
        let reply = match msg::load::<Request>() {
            Ok(request) => match request {
                Request::Receive(amount) => Self::handle_receive(amount).await,
                Request::Join(program_id) => Self::handle_join(program_id).await,
                Request::Report => Self::handle_report().await,
            },
            Err(e) => {
                debug!("Error processing request: {e:?}");
                Reply::Failure
            }
        };

        debug!("Handle request finished");
        msg::reply(reply, 0).unwrap();
    }

    async fn handle_receive(amount: u64) -> Reply {
        debug!("Handling receive {amount}");

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

            debug!("Set own amount to: {left_over}");
            *Self::amount() = *Self::amount() + left_over;
        } else {
            debug!("Set own amount to: {amount}");
            *Self::amount() = *Self::amount() + amount;
        }

        Reply::Success
    }

    async fn handle_join(program_id: ActorId) -> Reply {
        let mut nodes = Self::nodes().lock().await;
        debug!("Inserting into nodes");
        nodes.as_mut().insert(Program::new(program_id));
        Reply::Success
    }

    async fn handle_report() -> Reply {
        let mut amount = *Program::amount();
        debug!("Own amount: {amount}");

        let nodes = Program::nodes().lock().await;

        for program in nodes.as_ref().iter() {
            debug!("Querying next node");
            amount += match program.do_report().await {
                Ok(amount) => {
                    debug!("Sub-node result: {amount}");
                    amount
                }
                Err(_) => {
                    // skipping erroneous sub-nodes!
                    debug!("Skipping erroneous node");
                    0
                }
            }
        }

        Reply::Amount(amount)
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    debug!("Handling sequence started");
    gstd::message_loop(Program::handle_request());
    debug!("Handling sequence terminated");
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    gstd::handle_reply_with_hook();
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { STATE = Some(Default::default()) };
    msg::reply((), 0).unwrap();
    debug!("Program initialized");
}
