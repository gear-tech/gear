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

//! This `gstd` module provides async messaging functions.

use crate::async_runtime::{signals, ReplyPoll};
use crate::errors::{ContractError, Result};
use crate::prelude::{convert::AsRef, Vec};
use crate::{ActorId, MessageId};
use codec::{Decode, Encode};
use core::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::future::FusedFuture;

pub struct CodecMessageFuture<T> {
    waiting_reply_to: MessageId,
    phantom: PhantomData<T>,
}

impl<D: Decode> Future for CodecMessageFuture<D> {
    type Output = Result<D>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut self;
        match signals().poll(fut.waiting_reply_to, cx) {
            ReplyPoll::None => panic!("Somebody created CodecMessageFuture with the MessageId that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some((actual_reply, exit_code)) => {
                if exit_code != 0 {
                    return Poll::Ready(Err(ContractError::ExitCode(exit_code)));
                }

                Poll::Ready(D::decode(&mut actual_reply.as_ref()).map_err(ContractError::Decode))
            },
        }
    }
}

impl<D: Decode> FusedFuture for CodecMessageFuture<D> {
    fn is_terminated(&self) -> bool {
        !signals().waits_for(self.waiting_reply_to)
    }
}

/// To interrupt a program execution waiting for a reply on a previous message,
/// one need to call an `.await` expression.
/// The initial message that requires a reply is sent instantly.
/// Methods `send_and_wait_for_reply` return `MessageFuture` where `.await` can
/// be called. `MessageFuture` checks if the reply exists already:
///     if not, program interrupts
///     if reply exists, function checks it's exit code:
///          if it equals 0, returns `Ok(payload)`
///          else returns `Err(ContractError::ExitCode(code))`
pub struct MessageFuture {
    waiting_reply_to: MessageId,
}

impl Future for MessageFuture {
    type Output = Result<Vec<u8>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut *self;
        match signals().poll(fut.waiting_reply_to, cx) {
            ReplyPoll::None => panic!("Somebody created MessageFuture with the MessageId that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some((actual_reply, exit_code)) => {
                if exit_code != 0 {
                    return Poll::Ready(Err(ContractError::ExitCode(exit_code)));
                }

                Poll::Ready(Ok(actual_reply))
            },
        }
    }
}

impl FusedFuture for MessageFuture {
    fn is_terminated(&self) -> bool {
        !signals().waits_for(self.waiting_reply_to)
    }
}

/// Send a message and wait for reply (as encoded structure).
pub fn send_and_wait_for_reply<D: Decode, E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> CodecMessageFuture<D> {
    let waiting_reply_to = crate::msg::send(program, payload, gas_limit, value);
    signals().register_signal(waiting_reply_to);

    CodecMessageFuture::<D> {
        waiting_reply_to,
        phantom: PhantomData,
    }
}

/// Send a message and wait for reply (as bytes).
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
