// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! This module provides useful functions for working with event stream.

use crate::{
    Error, Event, Result,
    gear::{gear, runtime_types::gear_common::event::DispatchStatus},
};
use futures::prelude::*;
use gear_core::ids::MessageId;
use parity_scale_codec::Decode;
use std::{
    collections::{HashMap, HashSet},
    pin::pin,
};

/// Extracts a reply on given message from a stream of events.
///
/// The reply is returned as bytes. If decoding the reply payload
/// is needed, see [`reply_on`].
pub async fn reply_bytes_on(
    message_id: MessageId,
    events: impl Stream<Item = Result<Event>>,
) -> Result<Result<Vec<u8>, Vec<u8>>> {
    let payloads = events
        .map(|event| {
            Ok::<_, Error>(
                if let Event::Gear(gear::Event::UserMessageSent { message, .. }) = event?
                    && let Some(details) = message.details()
                    && details.to_message_id() == message_id
                {
                    let payload = message.payload_bytes().to_vec();

                    if details.to_reply_code().is_success() {
                        Some(Ok(payload))
                    } else {
                        Some(Err(payload))
                    }
                } else {
                    None
                },
            )
        })
        .filter_map(|res| future::ready(res.transpose()));

    pin!(payloads)
        .next()
        .await
        .unwrap_or(Err(Error::EventNotFound))
}

/// Extracts a reply on given message from a stream of events.
///
/// The reply payload is decoded as `T` for success payload
/// and as [`String`] for error messages.
///
/// For more low-level variant of the function that doesn't do
/// decoding see [`reply_bytes_on`].
pub async fn reply_on<T: Decode>(
    message_id: MessageId,
    events: impl Stream<Item = Result<Event>>,
) -> Result<Result<T, String>> {
    Ok(match reply_bytes_on(message_id, events).await? {
        Ok(payload) => Ok(T::decode(&mut payload.as_slice())?),
        Err(payload) => Err(String::from_utf8(payload)?),
    })
}

/// Waits until the message with given ID is processed
/// and returns is [`DispatchStatus`].
pub async fn message_dispatch_status(
    message_id: MessageId,
    events: impl Stream<Item = Result<Event>>,
) -> Result<DispatchStatus> {
    let dispatch_statuses = events
        .map(|event| {
            Ok::<_, Error>(match event? {
                Event::Gear(gear::Event::MessagesDispatched { statuses, .. }) => statuses
                    .into_iter()
                    .find_map(|(mid, status)| (mid == message_id).then_some(status)),
                _ => None,
            })
        })
        .filter_map(|res| future::ready(res.transpose()));

    pin!(dispatch_statuses)
        .next()
        .await
        .unwrap_or(Err(Error::EventNotFound))
}

/// Waits until messages with given IDs are processed
/// and returns their [`DispatchStatus`]es.
pub async fn message_batch_dispatch_statuses(
    message_ids: impl IntoIterator<Item = MessageId>,
    events: impl Stream<Item = Result<Event>>,
) -> Result<HashMap<MessageId, DispatchStatus>> {
    let statuses = events
        .map(|event| {
            Ok::<_, Error>(
                stream::iter(match event? {
                    Event::Gear(gear::Event::MessagesDispatched { statuses, .. }) => {
                        Some(stream::iter(statuses).map(Ok::<_, Error>))
                    }
                    _ => None,
                })
                .map(Ok::<_, Error>),
            )
        })
        .try_flatten()
        .try_flatten();
    let mut statuses = pin!(statuses);

    let mut message_ids = HashSet::<MessageId>::from_iter(message_ids);
    let mut message_statuses = HashMap::new();

    while let Some((message_id, status)) = statuses.next().await.transpose()? {
        if message_ids.remove(&message_id) {
            message_statuses.insert(message_id, status);
        }

        if message_ids.is_empty() {
            return Ok(message_statuses);
        }
    }

    Err(Error::EventNotFound)
}
