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

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use gcore::{msg, MessageId, ProgramId};

static mut WAITING_MESSAGES: Option<BTreeMap<MessageId, MessageId>> = None;
static mut FUTURES: Option<BTreeMap<MessageId, MessageFutures>> = None;

#[derive(Clone, Default)]
pub struct MessageFuture {
    reply: Option<Vec<u8>>,
}

impl MessageFuture {
    pub fn new() -> Self {
        Self { reply: None }
    }

    pub fn set_reply(&mut self, payload: Vec<u8>) {
        self.reply = Some(payload);
    }

    pub fn is_empty(&self) -> bool {
        self.reply.is_none()
    }
}

impl Future for MessageFuture {
    type Output = Vec<u8>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut *self;
        if fut.reply.is_some() {
            Poll::Ready(fut.reply.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}

pub(crate) struct MessageFutures {
    current: usize,
    futures: Vec<MessageFuture>,
}

impl MessageFutures {
    pub fn new() -> Self {
        Self {
            current: 0,
            futures: Vec::new(),
        }
    }

    pub fn reset_current(&mut self) {
        self.current = 0;
    }

    pub fn clear(&mut self) {
        self.futures.clear();
    }

    pub fn current_future_mut(&mut self) -> &mut MessageFuture {
        while self.current >= self.futures.len() {
            self.futures.push(MessageFuture::new());
        }
        self.futures.get_mut(self.current).unwrap()
    }

    pub fn next_future(&mut self) -> &MessageFuture {
        self.current += 1;
        self.current_future_mut()
    }
}

/// Send a message and wait for reply.
pub fn send_and_wait_for_reply(
    program: ProgramId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> MessageFuture {
    let key_id = msg::id();
    let fut = futures(key_id).next_future();
    if fut.is_empty() {
        // New message
        let sent_msg_id = msg::send_with_value(program, payload, gas_limit, value);
        waiting_messages().insert(sent_msg_id, key_id);
        msg::wait();
    }

    fut.clone()
}

pub(crate) fn waiting_messages() -> &'static mut BTreeMap<MessageId, MessageId> {
    unsafe {
        if WAITING_MESSAGES.is_none() {
            WAITING_MESSAGES = Some(BTreeMap::new());
        }
        WAITING_MESSAGES.as_mut().unwrap()
    }
}

pub(crate) fn futures(id: MessageId) -> &'static mut MessageFutures {
    unsafe {
        if FUTURES.is_none() {
            FUTURES = Some(BTreeMap::new());
        }
        let map = FUTURES
            .as_mut()
            .expect("Cannot be none as it is set above; qed");
        map.entry(id).or_insert_with(MessageFutures::new)
    }
}
