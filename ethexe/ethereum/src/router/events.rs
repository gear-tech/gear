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

use crate::{
    IRouter,
    abi::utils::{bytes32_to_code_id, bytes32_to_h256},
    decode_log,
    router::RouterQuery,
};
use alloy::{
    contract::Event,
    primitives::B256,
    providers::{Provider, RootProvider},
    rpc::types::{Filter, Log, Topic},
    sol_types::{Error, SolEvent},
};
use anyhow::{Result, anyhow};
use ethexe_common::events::{
    RouterEvent, RouterRequestEvent,
    router::{
        AnnouncesCommittedEvent, BatchCommittedEvent, CodeGotValidatedEvent,
        CodeValidationRequestedEvent, ComputationSettingsChangedEvent, ProgramCreatedEvent,
        StorageSlotChangedEvent, ValidatorsCommittedForEraEvent,
    },
};
use futures::{Stream, StreamExt};
use gprimitives::CodeId;
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IRouter;
        BATCH_COMMITTED: BatchCommitted,
        ANNOUNCES_COMMITTED: AnnouncesCommitted,
        CODE_GOT_VALIDATED: CodeGotValidated,
        CODE_VALIDATION_REQUESTED: CodeValidationRequested,
        COMPUTATION_SETTINGS_CHANGED: ComputationSettingsChanged,
        PROGRAM_CREATED: ProgramCreated,
        STORAGE_SLOT_CHANGED: StorageSlotChanged,
        VALIDATORS_COMMITTED_FOR_ERA: ValidatorsCommittedForEra,
    }

    pub const REQUESTS: &[B256] = &[
        CODE_VALIDATION_REQUESTED,
        COMPUTATION_SETTINGS_CHANGED,
        PROGRAM_CREATED,
        STORAGE_SLOT_CHANGED,
        VALIDATORS_COMMITTED_FOR_ERA,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<RouterEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        BATCH_COMMITTED => {
            RouterEvent::BatchCommitted(decode_log::<IRouter::BatchCommitted>(log)?.into())
        }
        ANNOUNCES_COMMITTED => {
            RouterEvent::AnnouncesCommitted(decode_log::<IRouter::AnnouncesCommitted>(log)?.into())
        }
        CODE_GOT_VALIDATED => {
            RouterEvent::CodeGotValidated(decode_log::<IRouter::CodeGotValidated>(log)?.into())
        }
        CODE_VALIDATION_REQUESTED => {
            let tx_hash = log
                .transaction_hash
                .ok_or_else(|| anyhow!("Tx hash not found"))?;
            let block_timestamp = log
                .block_timestamp
                .ok_or_else(|| anyhow!("Block timestamp not found"))?;
            let event = decode_log::<IRouter::CodeValidationRequested>(log)?;

            RouterEvent::CodeValidationRequested(CodeValidationRequestedEvent {
                code_id: bytes32_to_code_id(event.codeId),
                timestamp: block_timestamp,
                tx_hash: bytes32_to_h256(tx_hash),
            })
        }
        COMPUTATION_SETTINGS_CHANGED => RouterEvent::ComputationSettingsChanged(
            decode_log::<IRouter::ComputationSettingsChanged>(log)?.into(),
        ),
        PROGRAM_CREATED => {
            RouterEvent::ProgramCreated(decode_log::<IRouter::ProgramCreated>(log)?.into())
        }
        STORAGE_SLOT_CHANGED => {
            RouterEvent::StorageSlotChanged(decode_log::<IRouter::StorageSlotChanged>(log)?.into())
        }
        VALIDATORS_COMMITTED_FOR_ERA => RouterEvent::ValidatorsCommittedForEra(
            decode_log::<IRouter::ValidatorsCommittedForEra>(log)?.into(),
        ),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_request_event(log: &Log) -> Result<Option<RouterRequestEvent>> {
    if log.topic0().filter(|&v| REQUESTS.contains(v)).is_none() {
        return Ok(None);
    }

    let request_event = try_extract_event(log)?
        .and_then(|v| v.to_request())
        .expect("filtered above");

    Ok(Some(request_event))
}

pub struct AllEventsBuilder<'a> {
    query: &'a RouterQuery,
}

impl<'a> AllEventsBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self { query }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<RouterEvent>> + Unpin + use<>> {
        let filter = Filter::new()
            .address(*self.query.instance.address())
            .event_signature(Topic::from_iter([
                signatures::BATCH_COMMITTED,
                signatures::ANNOUNCES_COMMITTED,
                signatures::CODE_GOT_VALIDATED,
                signatures::CODE_VALIDATION_REQUESTED,
                signatures::COMPUTATION_SETTINGS_CHANGED,
                signatures::PROGRAM_CREATED,
                signatures::STORAGE_SLOT_CHANGED,
                signatures::VALIDATORS_COMMITTED_FOR_ERA,
            ]));
        Ok(self
            .query
            .instance
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream()
            .map(|log| try_extract_event(&log).transpose().expect("infallible")))
    }
}

pub struct BatchCommittedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::BatchCommitted>,
}

impl<'a> BatchCommittedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.BatchCommitted_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(BatchCommittedEvent, Log), Error>> + Unpin + use<>> {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct AnnouncesCommittedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::AnnouncesCommitted>,
}

impl<'a> AnnouncesCommittedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.AnnouncesCommitted_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(AnnouncesCommittedEvent, Log), Error>> + Unpin + use<>>
    {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct CodeGotValidatedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::CodeGotValidated>,
    valid: Option<bool>,
}

impl<'a> CodeGotValidatedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.CodeGotValidated_filter(),
            valid: None,
        }
    }

    pub fn valid(mut self, valid: bool) -> Self {
        self.valid = Some(valid);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(CodeGotValidatedEvent, Log), Error>> + Unpin + use<>>
    {
        let mut event = self.event;
        if let Some(valid) = self.valid {
            event = event.topic1(valid);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct CodeValidationRequestedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::CodeValidationRequested>,
}

impl<'a> CodeValidationRequestedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.CodeValidationRequested_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<
        impl Stream<Item = Result<(CodeValidationRequestedEvent, Log), Error>> + Unpin + use<>,
    > {
        Ok(self.event.subscribe().await?.into_stream().map(|result| {
            result.and_then(|(event, log)| {
                let tx_hash = log
                    .transaction_hash
                    .ok_or_else(|| Error::Other("Tx hash not found".into()))?;
                let block_timestamp = log
                    .block_timestamp
                    .ok_or_else(|| Error::Other("Block timestamp not found".into()))?;
                let event = CodeValidationRequestedEvent {
                    code_id: bytes32_to_code_id(event.codeId),
                    timestamp: block_timestamp,
                    tx_hash: bytes32_to_h256(tx_hash),
                };
                Ok((event, log))
            })
        }))
    }
}

pub struct ValidatorsCommittedForEraEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::ValidatorsCommittedForEra>,
}

impl<'a> ValidatorsCommittedForEraEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.ValidatorsCommittedForEra_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<
        impl Stream<Item = Result<(ValidatorsCommittedForEraEvent, Log), Error>> + Unpin + use<>,
    > {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ComputationSettingsChangedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::ComputationSettingsChanged>,
}

impl<'a> ComputationSettingsChangedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.ComputationSettingsChanged_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<
        impl Stream<Item = Result<(ComputationSettingsChangedEvent, Log), Error>> + Unpin + use<>,
    > {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct ProgramCreatedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::ProgramCreated>,
    code_id: Option<CodeId>,
}

impl<'a> ProgramCreatedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.ProgramCreated_filter(),
            code_id: None,
        }
    }

    pub fn code_id(mut self, code_id: CodeId) -> Self {
        self.code_id = Some(code_id);
        self
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(ProgramCreatedEvent, Log), Error>> + Unpin + use<>> {
        let mut event = self.event;
        if let Some(code_id) = self.code_id {
            let code_id: B256 = code_id.into_bytes().into();
            event = event.topic1(code_id);
        }
        Ok(event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

pub struct StorageSlotChangedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::StorageSlotChanged>,
}

impl<'a> StorageSlotChangedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.StorageSlotChanged_filter(),
        }
    }

    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(StorageSlotChangedEvent, Log), Error>> + Unpin + use<>>
    {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}
