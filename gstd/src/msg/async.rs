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

//! This `gstd` module provides async messaging functions.

use crate::{
    async_runtime::{signals, ReplyPoll},
    errors::{ContractError, Result},
    prelude::{convert::AsRef, Vec},
    ActorId, MessageId,
};
use codec::{Decode, Encode};
use core::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::future::FusedFuture;

/// To interrupt a program execution waiting for a reply on a previous message,
/// one needs to call an `.await` expression.
/// The initial message that requires a reply is sent instantly.
/// Function `send_for_reply` returns `CodecMessageFuture` which
/// implements `Future` trait. Program interrupts until the reply is received.
/// As soon as the reply is received, the function checks it's exit code and
/// returns `Ok()` with decoded structure inside or `Err()` in case of exit code
/// does not equal 0. For decode-related errors (<https://docs.rs/parity-scale-codec/2.3.1/parity_scale_codec/struct.Error.html>),
/// Gear returns the native one after decode.
pub struct CodecMessageFuture<T> {
    /// Waiting reply to this the message id
    pub waiting_reply_to: MessageId,
    /// Marker
    _marker: PhantomData<T>,
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
/// one needs to call an `.await` expression.
/// The initial message that requires a reply is sent instantly.
/// Function `send_bytes_for_reply` returns `MessageFuture` which
/// implements `Future` trait. Program interrupts until the reply is received.
/// As soon as the reply is received, the function checks it's exit code and
/// returns `Ok()` with raw bytes inside or `Err()` in case of exit code does
/// not equal 0. For decode-related errors (<https://docs.rs/parity-scale-codec/2.3.1/parity_scale_codec/struct.Error.html>),
/// Gear returns the native one after decode.
pub struct MessageFuture {
    /// Waiting reply to this the message id
    pub waiting_reply_to: MessageId,
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

/// # Warning
///
/// This function is deprecated, please use
/// [`send_for_reply`](crate::msg::basic::send_for_reply) instead.
///
/// Send a message and wait for reply.
///
/// This function works similarly to `send_bytes_and_wait_for_reply`,
/// with one difference - it takes the structure in, then encodes it
/// and sends it in bytes. When receiving the message, it decodes the bytes.
/// So the input should be SCALE codeс encodable, output - decodable
/// (<https://docs.substrate.io/v3/advanced/scale-codec/>).
/// The program will be interrupted (waiting for a reply) if an `.await`
/// has been called on the `CodecMessageFuture` object returned by the function.
pub fn send_and_wait_for_reply<D: Decode, E: Encode>(
    program: ActorId,
    payload: E,
    value: u128,
) -> Result<CodecMessageFuture<D>> {
    let waiting_reply_to = crate::msg::send(program, payload, value)?;
    signals().register_signal(waiting_reply_to);

    Ok(CodecMessageFuture::<D> {
        waiting_reply_to,
        phantom: PhantomData,
    })
}

/// # Warning
///
/// This function is deprecated, please use
/// [`send_bytes_for_reply`](crate::msg::basic::send_bytes_for_reply) instead.
///
/// Send a message and wait for reply.
///
/// This function works similarly to `send_and_wait_for_reply`,
/// with one difference - it works with raw bytes as a paylod.
/// The program will be interrupted (waiting for a reply) if an `.await`
/// has been called on the `MessageFuture` object returned by the function.
pub fn send_bytes_and_wait_for_reply<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    value: u128,
) -> Result<MessageFuture> {
    let waiting_reply_to = crate::msg::send_bytes(program, payload, value)?;
    signals().register_signal(waiting_reply_to);

    Ok(MessageFuture { waiting_reply_to })
}
