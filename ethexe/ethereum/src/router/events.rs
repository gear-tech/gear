// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
        BatchCommittedEvent, CodeGotValidatedEvent, CodeValidationRequestedEvent,
        ComputationSettingsChangedEvent, EBCommittedEvent, MBCommittedEvent, ProgramCreatedEvent,
        StorageSlotChangedEvent, ValidatorsCommittedForEraEvent,
    },
};
use futures::{Stream, StreamExt};
use gprimitives::CodeId;
use signatures::*;

/// Keccak-256 topic-0 signature hashes for every `IRouter` event, plus the `REQUESTS` and `ALL`
/// slices used to build log filters.
pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IRouter;
        BATCH_COMMITTED: BatchCommitted,
        MB_COMMITTED: MBCommitted,
        EB_COMMITTED: EBCommitted,
        CODE_GOT_VALIDATED: CodeGotValidated,
        CODE_VALIDATION_REQUESTED: CodeValidationRequested,
        COMPUTATION_SETTINGS_CHANGED: ComputationSettingsChanged,
        PROGRAM_CREATED: ProgramCreated,
        STORAGE_SLOT_CHANGED: StorageSlotChanged,
        VALIDATORS_COMMITTED_FOR_ERA: ValidatorsCommittedForEra,
    }

    /// Topic-0 signatures for events that originate from an on-chain user or validator request,
    /// as opposed to events that record execution outcomes.
    pub const REQUESTS: &[B256] = &[
        CODE_VALIDATION_REQUESTED,
        COMPUTATION_SETTINGS_CHANGED,
        PROGRAM_CREATED,
        STORAGE_SLOT_CHANGED,
        VALIDATORS_COMMITTED_FOR_ERA,
    ];
}

/// Attempts to decode an Ethereum log into a [`RouterEvent`].
///
/// Returns `Ok(None)` when the log's topic-0 does not match any known Router event signature,
/// and `Err` when the log matches but decoding fails.
pub fn try_extract_event(log: &Log) -> Result<Option<RouterEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        BATCH_COMMITTED => {
            RouterEvent::BatchCommitted(decode_log::<IRouter::BatchCommitted>(log)?.into())
        }
        MB_COMMITTED => RouterEvent::MBCommitted(decode_log::<IRouter::MBCommitted>(log)?.into()),
        EB_COMMITTED => RouterEvent::EBCommitted(decode_log::<IRouter::EBCommitted>(log)?.into()),
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

/// Attempts to decode an Ethereum log into a [`RouterRequestEvent`].
///
/// Returns `Ok(None)` when the log does not match a request-category event signature (see
/// `signatures::REQUESTS`), and `Err` when it matches but decoding fails.
pub fn try_extract_request_event(log: &Log) -> Result<Option<RouterRequestEvent>> {
    if log.topic0().filter(|&v| REQUESTS.contains(v)).is_none() {
        return Ok(None);
    }

    let request_event = try_extract_event(log)?
        .and_then(|v| v.to_request())
        .expect("filtered above");

    Ok(Some(request_event))
}

/// Builder that subscribes to all Router contract events at once.
///
/// Call [`AllEventsBuilder::subscribe`] to open a live stream of every [`RouterEvent`] variant
/// emitted by the Router contract address bound to the underlying [`RouterQuery`].
pub struct AllEventsBuilder<'a> {
    query: &'a RouterQuery,
}

impl<'a> AllEventsBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self { query }
    }

    /// Subscribes to all Router events and returns a live stream of decoded [`RouterEvent`]s.
    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<RouterEvent>> + Unpin + use<>> {
        let filter = Filter::new()
            .address(*self.query.instance.address())
            .event_signature(Topic::from_iter([
                signatures::BATCH_COMMITTED,
                signatures::MB_COMMITTED,
                signatures::EB_COMMITTED,
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

/// Builder that subscribes to `BatchCommitted` events from the Router contract.
pub struct BatchCommittedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::BatchCommitted>,
}

impl<'a> BatchCommittedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.BatchCommitted_filter(),
        }
    }

    /// Subscribes to `BatchCommitted` events and returns a live stream of decoded event/log pairs.
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

/// Builder that subscribes to `MBCommitted` (announces-chain head committed) events from the Router contract.
pub struct MBCommittedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::MBCommitted>,
}

impl<'a> MBCommittedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.MBCommitted_filter(),
        }
    }

    /// Subscribes to `MBCommitted` events and returns a live stream of decoded event/log pairs.
    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(MBCommittedEvent, Log), Error>> + Unpin + use<>> {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

/// Builder that subscribes to `EBCommitted` (Ethereum-block committed) events from the Router contract.
pub struct EBCommittedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::EBCommitted>,
}

impl<'a> EBCommittedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.EBCommitted_filter(),
        }
    }

    /// Subscribes to `EBCommitted` events and returns a live stream of decoded event/log pairs.
    pub async fn subscribe(
        self,
    ) -> Result<impl Stream<Item = Result<(EBCommittedEvent, Log), Error>> + Unpin + use<>> {
        Ok(self
            .event
            .subscribe()
            .await?
            .into_stream()
            .map(|result| result.map(|(event, log)| (event.into(), log))))
    }
}

/// Builder that subscribes to `CodeGotValidated` events from the Router contract.
///
/// Optionally filter by validation outcome using [`CodeGotValidatedEventBuilder::valid`].
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

    /// Restricts the subscription to events where the `valid` indexed field equals `valid`.
    pub fn valid(mut self, valid: bool) -> Self {
        self.valid = Some(valid);
        self
    }

    /// Subscribes to `CodeGotValidated` events and returns a live stream of decoded event/log pairs.
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

/// Builder that subscribes to `CodeValidationRequested` events from the Router contract.
pub struct CodeValidationRequestedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::CodeValidationRequested>,
}

impl<'a> CodeValidationRequestedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.CodeValidationRequested_filter(),
        }
    }

    /// Subscribes to `CodeValidationRequested` events and returns a live stream of decoded event/log pairs.
    ///
    /// The stream enriches each event with the originating transaction hash and block timestamp,
    /// returning an error if either field is absent from the log.
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

/// Builder that subscribes to `ValidatorsCommittedForEra` events from the Router contract.
pub struct ValidatorsCommittedForEraEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::ValidatorsCommittedForEra>,
}

impl<'a> ValidatorsCommittedForEraEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.ValidatorsCommittedForEra_filter(),
        }
    }

    /// Subscribes to `ValidatorsCommittedForEra` events and returns a live stream of decoded event/log pairs.
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

/// Builder that subscribes to `ComputationSettingsChanged` events from the Router contract.
pub struct ComputationSettingsChangedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::ComputationSettingsChanged>,
}

impl<'a> ComputationSettingsChangedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.ComputationSettingsChanged_filter(),
        }
    }

    /// Subscribes to `ComputationSettingsChanged` events and returns a live stream of decoded event/log pairs.
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

/// Builder that subscribes to `ProgramCreated` events from the Router contract.
///
/// Optionally filter by the code identifier using [`ProgramCreatedEventBuilder::code_id`].
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

    /// Restricts the subscription to events where the `codeId` indexed field matches `code_id`.
    pub fn code_id(mut self, code_id: CodeId) -> Self {
        self.code_id = Some(code_id);
        self
    }

    /// Subscribes to `ProgramCreated` events and returns a live stream of decoded event/log pairs.
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

/// Builder that subscribes to `StorageSlotChanged` events from the Router contract.
pub struct StorageSlotChangedEventBuilder<'a> {
    event: Event<&'a RootProvider, IRouter::StorageSlotChanged>,
}

impl<'a> StorageSlotChangedEventBuilder<'a> {
    pub(crate) fn new(query: &'a RouterQuery) -> Self {
        Self {
            event: query.instance.StorageSlotChanged_filter(),
        }
    }

    /// Subscribes to `StorageSlotChanged` events and returns a live stream of decoded event/log pairs.
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
