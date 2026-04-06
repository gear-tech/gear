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

use crate::{IWrappedVara, decode_log, wvara::WVaraQuery};
use alloy::{
    contract::Event,
    primitives::{Address as AlloyAddress, B256},
    providers::{Provider, RootProvider},
    rpc::types::{Filter, Log, Topic},
    sol_types::{Error, SolEvent},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    events::{
        WVaraEvent,
        wvara::{ApprovalEvent, TransferEvent},
    },
};
use futures::{Stream, StreamExt};
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IWrappedVara;
        TRANSFER: Transfer,
        APPROVAL: Approval,
    }

    pub const REQUESTS: &[B256] = &[TRANSFER];
}

pub fn try_extract_event(log: &Log) -> Result<Option<WVaraEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        TRANSFER => WVaraEvent::Transfer(decode_log::<IWrappedVara::Transfer>(log)?.into()),
        APPROVAL => WVaraEvent::Approval(decode_log::<IWrappedVara::Approval>(log)?.into()),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub struct AllEventsBuilder<'a> {
    query: &'a WVaraQuery,
}

impl<'a> AllEventsBuilder<'a> {
    pub(crate) fn new(query: &'a WVaraQuery) -> Self {
        Self { query }
    }

    pub async fn subscribe(self) -> Result<impl Stream<Item = Result<WVaraEvent>> + Unpin + use<>> {
        let filter = Filter::new()
            .address(*self.query.0.address())
            .event_signature(Topic::from_iter([
                signatures::TRANSFER,
                signatures::APPROVAL,
            ]));
        Ok(self
            .query
            .0
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream()
            .map(|log| try_extract_event(&log).transpose().expect("infallible")))
    }
}

pub struct TransferEventBuilder<'a> {
    event: Event<&'a RootProvider, IWrappedVara::Transfer>,
    from: Option<Address>,
    to: Option<Address>,
}

impl<'a> TransferEventBuilder<'a> {
    pub(crate) fn new(query: &'a WVaraQuery) -> Self {
        Self {
            event: query.0.Transfer_filter(),
            from: None,
            to: None,
        }
    }

    pub fn from(mut self, from: Address) -> Self {
        self.from = Some(from);
        self
    }

    pub fn to(mut self, to: Address) -> Self {
        self.to = Some(to);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(TransferEvent, Log), Error>> + Unpin + use<>> {
        let mut event = self.event;
        if let Some(from) = self.from {
            let from: AlloyAddress = from.into();
            event = event.topic1(from);
        }
        if let Some(to) = self.to {
            let to: AlloyAddress = to.into();
            event = event.topic2(to);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ApprovalEventBuilder<'a> {
    event: Event<&'a RootProvider, IWrappedVara::Approval>,
    owner: Option<Address>,
    spender: Option<Address>,
}

impl<'a> ApprovalEventBuilder<'a> {
    pub(crate) fn new(query: &'a WVaraQuery) -> Self {
        Self {
            event: query.0.Approval_filter(),
            owner: None,
            spender: None,
        }
    }

    pub fn owner(mut self, owner: Address) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn spender(mut self, spender: Address) -> Self {
        self.spender = Some(spender);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(ApprovalEvent, Log), Error>> + Unpin + use<>> {
        let mut event = self.event;
        if let Some(owner) = self.owner {
            let owner: AlloyAddress = owner.into();
            event = event.topic1(owner);
        }
        if let Some(spender) = self.spender {
            let spender: AlloyAddress = spender.into();
            event = event.topic2(spender);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}
