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
    async_runtime::{self, signals, Lock, ReplyPoll},
    errors::{ContractError, Result},
    prelude::{convert::AsRef, Vec},
    ActorId, MessageId,
};
use codec::Decode;
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
    ///
    /// # Note
    ///
    /// Need to `pub` this field because we are constructing this
    /// field in other files
    pub(crate) _marker: PhantomData<T>,
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

impl<D: Decode> CodecMessageFuture<D> {
    /// Delays handling for given specific amount of blocks.

    pub fn up_to(self, duration: u32) -> Self {
        async_runtime::locks().insert(crate::msg::id(), Lock::up_to(duration));
        self
    }

    /// Delays handling for maximal amount of blocks that could be payed, that
    /// doesn't exceed given duration.
    pub fn exactly(self, duration: u32) -> Self {
        async_runtime::locks().insert(crate::msg::id(), Lock::exactly(duration));
        self
    }
}

impl<D: Decode> FusedFuture for CodecMessageFuture<D> {
    fn is_terminated(&self) -> bool {
        !signals().waits_for(self.waiting_reply_to)
    }
}

/// Same as [`CodecMessageFuture`], but also contains program id
/// for functions that create programs.
pub struct CodecCreateProgramFuture<T> {
    /// Waiting reply to this the message id.
    pub waiting_reply_to: MessageId,
    /// Id of newly created program.
    pub program_id: ActorId,
    /// Marker
    ///
    /// # Note
    ///
    /// Need to `pub` this field because we are constructing this
    /// field in other files.
    pub(crate) _marker: PhantomData<T>,
}

impl<D: Decode> Future for CodecCreateProgramFuture<D> {
    type Output = Result<(ActorId, D)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut self;
        match signals().poll(fut.waiting_reply_to, cx) {
            ReplyPoll::None => panic!("Somebody created CodecCreateProgramFuture with the MessageId that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some((actual_reply, exit_code)) => {
                if exit_code != 0 {
                    return Poll::Ready(Err(ContractError::ExitCode(exit_code)));
                }

                Poll::Ready(D::decode(&mut actual_reply.as_ref()).map(|payload| (self.program_id, payload)).map_err(ContractError::Decode))
            },
        }
    }
}

impl<D: Decode> FusedFuture for CodecCreateProgramFuture<D> {
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
        // crate::debug!("\n\n polling");

        // check if message is timeout
        if let Some((expected, now)) = async_runtime::locks()
            .get(&fut.waiting_reply_to)
            .map(|lock| lock.timeout())
            .flatten()
        {
            return Poll::Ready(Err(ContractError::Timeout(expected, now)));
        } else {
            // crate::debug!("\n\n not timeout {:?}", fut.waiting_reply_to);
        }

        // do polling
        match signals().poll(fut.waiting_reply_to, cx) {
            ReplyPoll::None => panic!("Somebody created MessageFuture with the MessageId that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some((actual_reply, exit_code)) => {
                if exit_code != 0 {
                    return Poll::Ready(Err(ContractError::ExitCode(exit_code)));
                }

                // Remove lock after waking.
                async_runtime::locks().remove(&crate::msg::id());

                Poll::Ready(Ok(actual_reply))
            },
        }
    }
}

impl MessageFuture {
    /// Delays handling for given specific amount of blocks.
    pub fn up_to(self, duration: u32) -> Self {
        let locks = async_runtime::locks();
        let msg_id = crate::msg::id();
        if let Some(_) = locks.get(&msg_id) {
            // crate::debug!("\n\n resetting locks {}", duration);
        } else {
            // async_runtime::locks().insert(crate::msg::id(), Lock::up_to(duration));
        }
        async_runtime::locks().insert(crate::msg::id(), Lock::up_to(duration));
        self
    }

    /// Delays handling for maximal amount of blocks that could be payed, that
    /// doesn't exceed given duration.
    pub fn exactly(self, duration: u32) -> Self {
        async_runtime::locks().insert(crate::msg::id(), Lock::exactly(duration));
        self
    }
}

impl FusedFuture for MessageFuture {
    fn is_terminated(&self) -> bool {
        !signals().waits_for(self.waiting_reply_to)
    }
}

/// Same as [`MessageFuture`], but also contains program id
/// for functions that create programs.

pub struct CreateProgramFuture {
    /// Waiting reply to this the message id
    pub waiting_reply_to: MessageId,
    /// Id of newly created program.
    pub program_id: ActorId,
}

impl Future for CreateProgramFuture {
    type Output = Result<(ActorId, Vec<u8>)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = &mut *self;
        match signals().poll(fut.waiting_reply_to, cx) {
            ReplyPoll::None => panic!("Somebody created CreateProgramFuture with the MessageId that never ended in static replies!"),
            ReplyPoll::Pending => Poll::Pending,
            ReplyPoll::Some((actual_reply, exit_code)) => {
                if exit_code != 0 {
                    return Poll::Ready(Err(ContractError::ExitCode(exit_code)));
                }

                Poll::Ready(Ok((self.program_id, actual_reply)))
            },
        }
    }
}

impl FusedFuture for CreateProgramFuture {
    fn is_terminated(&self) -> bool {
        !signals().waits_for(self.waiting_reply_to)
    }
}
