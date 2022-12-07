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

use crate::{
    async_runtime::{self, signals, Lock, ReplyPoll},
    errors::{ContractError, Result},
    msg::macros::impl_futures,
    prelude::{convert::AsRef, Vec},
    ActorId, Config, MessageId,
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
    let msg_id = crate::msg::id();

    // check if message is timed out.
    if let Some((expected, now)) = async_runtime::locks().is_timeout(msg_id, waiting_reply_to) {
        // Remove lock after timeout.
        async_runtime::locks().remove(msg_id, waiting_reply_to);

        return Poll::Ready(Err(ContractError::Timeout(expected, now)));
    }

    match signals().poll(waiting_reply_to, cx) {
        ReplyPoll::None => panic!(
            "Somebody created a future with the MessageId that never ended in static replies!"
        ),
        ReplyPoll::Pending => Poll::Pending,
        ReplyPoll::Some((actual_reply, status_code)) => {
            // Remove lock after waking.
            async_runtime::locks().remove(msg_id, waiting_reply_to);

            if status_code != 0 {
                return Poll::Ready(Err(ContractError::StatusCode(status_code)));
            }

            Poll::Ready(f(actual_reply))
        }
    }
}

/// Same as [`MessageFuture`], but allows decoding the reply's payload instead
/// of getting a byte vector.
///
/// Generic `T` type should implement the [`Decode`] trait.
///
/// # Examples
///
/// In the following example, variable types are annotated explicitly for
/// demonstration purposes only. Usually, annotating them is unnecessary because
/// they can be inferred automatically.
///
/// ```
/// use gstd::{
///     msg::{self, CodecMessageFuture},
///     prelude::*,
/// };
/// # use gstd::ActorId;
///
/// #[derive(Decode)]
/// #[codec(crate = gstd::codec)]
/// struct Reply {
///     field: String,
/// }
///
/// #[gstd::async_main]
/// async fn main() {
///     # let dest = ActorId::zero();
///     let future: CodecMessageFuture<Reply> =
///         msg::send_bytes_for_reply_as(dest, b"PING", 0).expect("Unable to send");
///     let reply: Reply = future.await.expect("Unable to get a reply");
///     let field: String = reply.field;
/// }
///
/// # fn main() {}
/// ```
pub struct CodecMessageFuture<T> {
    /// An identifier of a message to which a reply is waited for.
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

/// Same as [`CreateProgramFuture`], but allows decoding the reply's payload
/// instead of getting a byte vector.
///
/// Generic `T` type should implement the [`Decode`] trait.
///
/// # Examples
///
/// In the following example, variable types are annotated explicitly for
/// demonstration purposes only. Usually, annotating them is unnecessary because
/// they can be inferred automatically.
///
/// ```
/// use gstd::{msg::CodecCreateProgramFuture, prelude::*, prog, ActorId};
/// # use gstd::CodeId;
///
/// #[derive(Decode)]
/// #[codec(crate = gstd::codec)]
/// struct InitReply {
///     field: String,
/// }
///
/// #[gstd::async_main]
/// async fn main() {
///     # let code_id = CodeId::new([0; 32]);
///     let future: CodecCreateProgramFuture<InitReply> =
///         prog::create_program_for_reply_as(code_id, b"salt", b"PING", 0)
///             .expect("Unable to create a program");
///     let (prog_id, reply): (ActorId, InitReply) = future.await.expect("Unable to get a reply");
///     let field: String = reply.field;
/// }
///
/// # fn main() {}
/// ```
pub struct CodecCreateProgramFuture<T> {
    /// An identifier of a message to which a reply is waited for.
    pub waiting_reply_to: MessageId,
    /// An identifier of a newly created program.
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

/// Future returned by async functions related to message sending that wait for
/// a reply (see sending functions with `_for_reply` suffix, e.g.
/// [`send_bytes_for_reply`](super::send_bytes_for_reply)).
///
/// To get the reply payload (in bytes), one should use `.await` syntax. After
/// calling a corresponding async function, the program interrupts its execution
/// until a reply arrives.
///
/// This future keeps the sent message identifier ([`MessageId`] to wake the
/// program after a reply arrives.
///
/// # Examples
///
/// In the following example, variable types are annotated explicitly for
/// demonstration purposes only. Usually, annotating them is unnecessary because
/// they can be inferred automatically.
///
/// ```
/// use gstd::msg::{self, MessageFuture};
/// # use gstd::ActorId;
///
/// #[gstd::async_main]
/// async fn main() {
///     # let dest = ActorId::zero();
///     let future: MessageFuture =
///         msg::send_bytes_for_reply(dest, b"PING", 0).expect("Unable to send");
///     let reply: Vec<u8> = future.await.expect("Unable to get a reply");
/// }
///
/// # fn main() {}
/// ```
pub struct MessageFuture {
    /// An identifier of a message to which a reply is waited for.
    ///
    /// This identifier is generated by the corresponding send function (e.g.
    /// [`send_bytes`](super::send_bytes)).
    pub waiting_reply_to: MessageId,
}

impl_futures!(
    MessageFuture,
    Vec<u8>,
    |fut, cx| => {
        poll(fut.waiting_reply_to, cx, Ok)
    }
);

/// Future returned by async functions related to program creating that wait for
/// a reply from the newly created program's init function (see program creating
/// functions with `_for_reply` suffix, e.g.
/// [`create_program_for_reply`](crate::prog::create_program_for_reply)).
///
/// To get the reply payload (in bytes), one should use `.await` syntax. After
/// calling a corresponding async function, the program interrupts its execution
/// until a reply arrives.
///
/// This future keeps the sent message identifier ([`MessageId`]) to wake the
/// program after a reply arrives. Also, it keeps an identifier of a newly
/// created program ([`ActorId`]).
///
/// # Examples
///
/// In the following example, variable types are annotated explicitly for
/// demonstration purposes only. Usually, annotating them is unnecessary because
/// they can be inferred automatically.
///
/// ```
/// use gstd::{msg::CreateProgramFuture, prog, ActorId};
/// # use gstd::CodeId;
///
/// #[gstd::async_main]
/// async fn main() {
///     # let code_id = CodeId::new([0; 32]);
///     let future: CreateProgramFuture =
///         prog::create_program_for_reply(code_id, b"salt", b"PING", 0)
///             .expect("Unable to create a program");
///     let (prog_id, reply): (ActorId, Vec<u8>) = future.await.expect("Unable to get a reply");
/// }
///
/// # fn main() {}
/// ```
pub struct CreateProgramFuture {
    /// An identifier of a message to which a reply is waited for.
    pub waiting_reply_to: MessageId,
    /// An identifier of a newly created program.
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
