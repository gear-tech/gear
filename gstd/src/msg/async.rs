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
    msg::macros::impl_futures,
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

fn poll<F, R>(waiting_reply_to: MessageId, cx: &mut Context<'_>, f: F) -> Poll<Result<R>>
where
    F: Fn(Vec<u8>) -> Result<R>,
{
    match signals().poll(waiting_reply_to, cx) {
        ReplyPoll::None => panic!(
            "Somebody created a future with the MessageId that never ended in static replies!"
        ),
        ReplyPoll::Pending => Poll::Pending,
        ReplyPoll::Some((actual_reply, status_code)) => {
            if status_code != 0 {
                return Poll::Ready(Err(ContractError::StatusCode(status_code)));
            }

            Poll::Ready(f(actual_reply))
        }
    }
}

/// To interrupt a program execution waiting for a reply on a previous message,
/// one needs to call an `.await` expression.
/// The initial message that requires a reply is sent instantly.
/// Function `send_for_reply` returns `CodecMessageFuture` which
/// implements `Future` trait. Program interrupts until the reply is received.
/// As soon as the reply is received, the function checks it's status code and
/// returns `Ok()` with decoded structure inside or `Err()` in case of status
/// code does not equal 0. For decode-related errors (<https://docs.rs/parity-scale-codec/2.3.1/parity_scale_codec/struct.Error.html>),
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

impl_futures!(
    CodecMessageFuture,
    D,
    D,
    |fut, cx| => {
        poll(fut.waiting_reply_to, cx, |reply| {
            D::decode(&mut reply.as_ref()).map_err(ContractError::Decode)
        })
    }
);

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

impl_futures!(
    CodecCreateProgramFuture,
    D,
    (ActorId, D),
    |fut, cx| => {
        poll(fut.waiting_reply_to, cx, |reply| {
            D::decode(&mut reply.as_ref())
                .map(|payload| (fut.program_id, payload))
                .map_err(ContractError::Decode)
        })
    }
);

/// To interrupt a program execution waiting for a reply on a previous message,
/// one needs to call an `.await` expression.
/// The initial message that requires a reply is sent instantly.
/// Function `send_bytes_for_reply` returns `MessageFuture` which
/// implements `Future` trait. Program interrupts until the reply is received.
/// As soon as the reply is received, the function checks it's status code and
/// returns `Ok()` with raw bytes inside or `Err()` in case of status code does
/// not equal 0. For decode-related errors (<https://docs.rs/parity-scale-codec/2.3.1/parity_scale_codec/struct.Error.html>),
/// Gear returns the native one after decode.
pub struct MessageFuture {
    /// Waiting reply to this the message id
    pub waiting_reply_to: MessageId,
}

impl_futures!(
    MessageFuture,
    Vec<u8>,
    |fut, cx| => {
        poll(fut.waiting_reply_to, cx, Ok)
    }
);

/// Same as [`MessageFuture`], but also contains program id
/// for functions that create programs.
pub struct CreateProgramFuture {
    /// Waiting reply to this the message id
    pub waiting_reply_to: MessageId,
    /// Id of newly created program.
    pub program_id: ActorId,
}

impl_futures!(
    CreateProgramFuture,
    (ActorId, Vec<u8>),
    |fut, cx| => {
        poll(fut.waiting_reply_to, cx, |reply| {
            Ok((fut.program_id, reply))
        })
    }
);
