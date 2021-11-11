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

use alloc::{collections::BTreeMap, vec::Vec};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use gcore::{msg, MessageId, ProgramId};

#[derive(Debug)]
struct WakeSignal {
    message_id: MessageId,
    payload: Option<Vec<u8>>,
}

pub(crate) struct WakeSignals {
    signals: BTreeMap<MessageId, WakeSignal>,
}

pub enum ReplyPoll {
    None,
    Pending,
    Some(Vec<u8>),
}

impl WakeSignals {
    pub(crate) fn new() -> Self {
        WakeSignals {
            signals: BTreeMap::new(),
        }
    }

    pub(crate) fn register_signal(
        &mut self,
        waiting_reply_to: MessageId,
        wake_this_message: MessageId,
    ) {
        self.signals.insert(
            waiting_reply_to,
            WakeSignal {
                message_id: wake_this_message,
                payload: None,
            },
        );
    }

    pub(crate) fn record_reply(&mut self, waiting_reply_to: MessageId, payload: Vec<u8>) {
        let mut signal = self
            .signals
            .get_mut(&waiting_reply_to)
            .expect("Somehow received reply for the message we never sent");

        signal.payload = Some(payload);
        gcore::exec::wake(signal.message_id, gcore::exec::gas_available());
    }

    pub(crate) fn poll(&mut self, message_reply_to: MessageId) -> ReplyPoll {
        match self.signals.remove(&message_reply_to) {
            None => ReplyPoll::None,
            Some(signal @ WakeSignal { payload: None, .. }) => {
                self.signals.insert(message_reply_to, signal);
                ReplyPoll::Pending
            }
            Some(WakeSignal {
                payload: Some(reply_payload),
                ..
            }) => ReplyPoll::Some(reply_payload),
        }
    }
}

static mut SIGNALS: Option<WakeSignals> = None;

pub(crate) fn signals_static() -> &'static mut WakeSignals {
    unsafe {
        if SIGNALS.as_ref().is_none() {
            SIGNALS = Some(WakeSignals::new());
        }

        SIGNALS.as_mut().expect("Created if none above; can't fail")
    }
}

pub struct MessageFuture {
    waiting_reply_to: MessageId,
}

impl Future for MessageFuture {
    type Output = Result<Vec<u8>, i32>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut *self;
        match signals_static().poll(fut.waiting_reply_to)        {
            ReplyPoll::None => panic!("Somebody created MessageFuture with the message_id that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some(actual_reply) => {
                let exit_code = gstd::msg::exit_code();
                if exit_code == 0 {
                    Poll::Ready(Ok(actual_reply))
                } else {
                    Poll::Ready(Err(exit_code))
                }
            },
        }
    }
}

/// Send a message and wait for reply.
pub fn send_and_wait_for_reply(
    program: ProgramId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> MessageFuture {
    let waiting_reply_to = msg::send(program, payload, gas_limit, value);
    signals_static().register_signal(waiting_reply_to, msg::id());

    MessageFuture { waiting_reply_to }
}
