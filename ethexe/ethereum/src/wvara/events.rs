// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

/// Keccak-256 topic-0 signature hashes for every `IWrappedVara` event, plus `REQUESTS` and `ALL`
/// slices used to build Ethereum log filters.
pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IWrappedVara;
        TRANSFER: Transfer,
        APPROVAL: Approval,
    }

    /// Subset of event signatures that represent user-initiated requests; currently only `Transfer`.
    pub const REQUESTS: &[B256] = &[TRANSFER];
}

/// Attempts to decode an Ethereum log into a [`WVaraEvent`].
///
/// Returns `Ok(None)` when the log's topic-0 does not match any known `IWrappedVara` event
/// signature, and `Err` when decoding a matching log fails.
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

/// Builder that subscribes to all `IWrappedVara` events (`Transfer` and `Approval`) on a given
/// contract address and yields them as a unified [`WVaraEvent`] stream.
pub struct AllEventsBuilder<'a> {
    query: &'a WVaraQuery,
}

impl<'a> AllEventsBuilder<'a> {
    pub(crate) fn new(query: &'a WVaraQuery) -> Self {
        Self { query }
    }

    /// Subscribes to all `IWrappedVara` log events and returns a stream of decoded [`WVaraEvent`]s.
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

/// Builder for subscribing to `IWrappedVara::Transfer` events, with optional indexed filters
/// on the `from` and `to` address topics.
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

    /// Restricts the subscription to transfers originating from `from`.
    pub fn from(mut self, from: Address) -> Self {
        self.from = Some(from);
        self
    }

    /// Restricts the subscription to transfers directed to `to`.
    pub fn to(mut self, to: Address) -> Self {
        self.to = Some(to);
        self
    }

    /// Subscribes with the configured topic filters and returns a stream of `(TransferEvent, Log)` pairs.
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

/// Builder for subscribing to `IWrappedVara::Approval` events, with optional indexed filters
/// on the `owner` and `spender` address topics.
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

    /// Restricts the subscription to approvals granted by `owner`.
    pub fn owner(mut self, owner: Address) -> Self {
        self.owner = Some(owner);
        self
    }

    /// Restricts the subscription to approvals granted to `spender`.
    pub fn spender(mut self, spender: Address) -> Self {
        self.spender = Some(spender);
        self
    }

    /// Subscribes with the configured topic filters and returns a stream of `(ApprovalEvent, Log)` pairs.
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
