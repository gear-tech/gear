// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{IMirror, decode_log, mirror::MirrorQuery};
use alloy::{
    contract::Event,
    primitives::{Address as AlloyAddress, B256},
    providers::RootProvider,
    rpc::types::eth::Log,
    sol_types::{Error, SolEvent},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    events::{
        MirrorEvent, MirrorRequestEvent,
        mirror::{
            MessageCallFailedEvent, MessageEvent, ReplyCallFailedEvent, ReplyEvent,
            StateChangedEvent, ValueClaimedEvent,
        },
    },
};
use futures::{Stream, StreamExt};
use gear_core::message::ReplyCode;
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IMirror;
        OWNED_BALANCE_TOP_UP_REQUESTED: OwnedBalanceTopUpRequested,
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED: ExecutableBalanceTopUpRequested,
        MESSAGE_QUEUEING_REQUESTED: MessageQueueingRequested,
        MESSAGE: Message,
        MESSAGE_CALL_FAILED: MessageCallFailed,
        REPLY_QUEUEING_REQUESTED: ReplyQueueingRequested,
        REPLY: Reply,
        REPLY_CALL_FAILED: ReplyCallFailed,
        STATE_CHANGED: StateChanged,
        VALUE_CLAIMED: ValueClaimed,
        VALUE_CLAIMING_REQUESTED: ValueClaimingRequested,
    }

    pub const REQUESTS: &[B256] = &[
        OWNED_BALANCE_TOP_UP_REQUESTED,
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED,
        MESSAGE_QUEUEING_REQUESTED,
        REPLY_QUEUEING_REQUESTED,
        VALUE_CLAIMING_REQUESTED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<MirrorEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        OWNED_BALANCE_TOP_UP_REQUESTED => MirrorEvent::OwnedBalanceTopUpRequested(
            decode_log::<IMirror::OwnedBalanceTopUpRequested>(log)?.into(),
        ),
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED => MirrorEvent::ExecutableBalanceTopUpRequested(
            decode_log::<IMirror::ExecutableBalanceTopUpRequested>(log)?.into(),
        ),
        MESSAGE_QUEUEING_REQUESTED => MirrorEvent::MessageQueueingRequested(
            decode_log::<IMirror::MessageQueueingRequested>(log)?.into(),
        ),
        MESSAGE => MirrorEvent::Message(decode_log::<IMirror::Message>(log)?.into()),
        MESSAGE_CALL_FAILED => {
            MirrorEvent::MessageCallFailed(decode_log::<IMirror::MessageCallFailed>(log)?.into())
        }
        REPLY_QUEUEING_REQUESTED => MirrorEvent::ReplyQueueingRequested(
            decode_log::<IMirror::ReplyQueueingRequested>(log)?.into(),
        ),
        REPLY => MirrorEvent::Reply(decode_log::<IMirror::Reply>(log)?.into()),
        REPLY_CALL_FAILED => {
            MirrorEvent::ReplyCallFailed(decode_log::<IMirror::ReplyCallFailed>(log)?.into())
        }
        STATE_CHANGED => {
            MirrorEvent::StateChanged(decode_log::<IMirror::StateChanged>(log)?.into())
        }
        VALUE_CLAIMED => {
            MirrorEvent::ValueClaimed(decode_log::<IMirror::ValueClaimed>(log)?.into())
        }
        VALUE_CLAIMING_REQUESTED => MirrorEvent::ValueClaimingRequested(
            decode_log::<IMirror::ValueClaimingRequested>(log)?.into(),
        ),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_request_event(log: &Log) -> Result<Option<MirrorRequestEvent>> {
    if log.topic0().filter(|&v| REQUESTS.contains(v)).is_none() {
        return Ok(None);
    }

    let request_event = try_extract_event(log)?
        .and_then(|v| v.to_request())
        .expect("filtered above");

    Ok(Some(request_event))
}

pub struct StateChangedEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::StateChanged>,
}

impl<'a> StateChangedEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.StateChanged_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(StateChangedEvent, Log), Error>> + Unpin + use<>> {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct MessageEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::Message>,
    destination: Option<Address>,
}

impl<'a> MessageEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.Message_filter(),
            destination: None,
        }
    }

    pub fn with_destination(mut self, destination: Address) -> Self {
        self.destination = Some(destination);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(MessageEvent, Log), Error>> + Unpin + use<>> {
        let mut event = self.event;
        if let Some(destination) = self.destination {
            let destination: AlloyAddress = destination.into();
            event = event.topic1(destination);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct MessageCallFailedEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::MessageCallFailed>,
    destination: Option<Address>,
}

impl<'a> MessageCallFailedEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.MessageCallFailed_filter(),
            destination: None,
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(MessageCallFailedEvent, Log), Error>> + Unpin + use<>>
    {
        let mut event = self.event;
        if let Some(destination) = self.destination {
            let destination: AlloyAddress = destination.into();
            event = event.topic1(destination);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ReplyEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::Reply>,
    reply_code: Option<ReplyCode>,
}

impl<'a> ReplyEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.Reply_filter(),
            reply_code: None,
        }
    }

    pub fn reply_code(mut self, reply_code: ReplyCode) -> Self {
        self.reply_code = Some(reply_code);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(ReplyEvent, Log), Error>> + Unpin + use<>> {
        let mut event = self.event;
        if let Some(reply_code) = self.reply_code {
            let mut bytes32 = [0u8; 32]; // TODO: check this
            bytes32[..4].copy_from_slice(&reply_code.to_bytes());
            event = event.topic1(bytes32);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ReplyCallFailedEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::ReplyCallFailed>,
    reply_code: Option<ReplyCode>,
}

impl<'a> ReplyCallFailedEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.ReplyCallFailed_filter(),
            reply_code: None,
        }
    }

    pub fn reply_code(mut self, reply_code: ReplyCode) -> Self {
        self.reply_code = Some(reply_code);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(ReplyCallFailedEvent, Log), Error>> + Unpin + use<>>
    {
        let mut event = self.event;
        if let Some(reply_code) = self.reply_code {
            let mut bytes32 = [0u8; 32]; // TODO: check this
            bytes32[..4].copy_from_slice(&reply_code.to_bytes());
            event = event.topic1(bytes32);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ValueClaimedEventBuilder<'a> {
    event: Event<&'a RootProvider, IMirror::ValueClaimed>,
}

impl<'a> ValueClaimedEventBuilder<'a> {
    pub(crate) fn new(query: &'a MirrorQuery) -> Self {
        Self {
            event: query.0.ValueClaimed_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(ValueClaimedEvent, Log), Error>> + Unpin + use<>> {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}
