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

use crate::async_runtime::{signals, ReplyPoll};
use crate::prelude::{convert::AsRef, Vec};
use crate::{ActorId, MessageId};
use codec::{Decode, Encode};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub struct MessageFuture {
    waiting_reply_to: MessageId,
}

impl Future for MessageFuture {
    type Output = Vec<u8>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut *self;
        match signals().poll(fut.waiting_reply_to) {
            ReplyPoll::None => panic!("Somebody created MessageFuture with the message_id that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some(actual_reply) => Poll::Ready(actual_reply),
        }
    }
}

use core::marker::PhantomData;

pub struct CodecMessageFuture<T> {
    waiting_reply_to: MessageId,
    phantom: PhantomData<T>,
}

impl<D: Decode> Future for CodecMessageFuture<D> {
    type Output = Result<D, codec::Error>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut self;
        match signals().poll(fut.waiting_reply_to)        {
            ReplyPoll::None => panic!("Somebody created MessageFuture with the message_id that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some(actual_reply) => Poll::Ready(D::decode(&mut actual_reply.as_ref())),
        }
    }
}

/// Send a message and wait for reply.
pub fn send_bytes_and_wait_for_reply<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    gas_limit: u64,
    value: u128,
) -> MessageFuture {
    let waiting_reply_to = crate::msg::send_bytes(program, payload, gas_limit, value);
    signals().register_signal(waiting_reply_to);

    MessageFuture { waiting_reply_to }
}

/// Send a message and wait for reply.
pub fn send_and_wait_for_reply<D: Decode, E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> CodecMessageFuture<D> {
    let waiting_reply_to = crate::msg::send_bytes(program, payload.encode(), gas_limit, value);
    signals().register_signal(waiting_reply_to);

    CodecMessageFuture::<D> {
        waiting_reply_to,
        phantom: PhantomData,
    }
}
