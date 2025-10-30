// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Gear-specific event streaming RPC.

use std::{collections::HashMap, marker::PhantomData, ops::RangeInclusive, sync::Arc};

use frame_support::{dispatch::Parameter, storage::storage_prefix};
use futures::{StreamExt, future::BoxFuture};
use gear_core::message::UserMessage;
use jsonrpsee::{
    PendingSubscriptionSink,
    core::{
        SubscriptionResult, async_trait,
        server::{DisconnectError, SubscriptionMessage, SubscriptionSink, TrySendError},
    },
    proc_macros::rpc,
    types::ErrorObjectOwned,
};
use log::{debug, error, warn};
use parity_scale_codec::Decode;
use parking_lot::Mutex;
use sc_client_api::{BlockchainEvents, StorageProvider, backend::Backend as ClientBackend};
use sc_rpc::SubscriptionTaskExecutor;
use sp_blockchain::HeaderBackend;
use sp_core::{Bytes, H256};
use sp_runtime::traits::{Header, SaturatedConversion, UniqueSaturatedInto};
use sp_storage::{StorageData, StorageKey};

use runtime_primitives::{Block, BlockNumber, Hash};

const MAX_PAYLOAD_PATTERN: usize = 256;
const MAX_BACKFILL_BLOCKS: u64 = 5_000;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct UserMsgFilter {
    pub source: Option<H256>,
    pub destination: Option<H256>,
    #[serde(default)]
    pub payload_filters: Vec<PayloadFilter>,
    pub from_block: Option<u64>,
    pub finalized_only: Option<bool>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct PayloadFilter {
    pub offset: u32,
    pub pattern: Bytes,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct UserMessageSentJson {
    pub block: Hash,
    pub index: u32,
    pub id: [u8; 32],
    pub source: [u8; 32],
    pub destination: [u8; 32],
    pub payload: Bytes,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<UserMessageReplyJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack: Option<bool>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct UserMessageReplyJson {
    pub to: [u8; 32],
    pub code_raw: Bytes,
    pub code: String,
}

impl UserMessageSentJson {
    fn from_message(block: Hash, index: u32, message: &UserMessage) -> Self {
        let reply = message.details().map(|details| {
            let (to, code) = details.into_parts();
            UserMessageReplyJson {
                to: to.into_bytes(),
                code_raw: Bytes(code.to_bytes().to_vec()),
                code: code.to_string(),
            }
        });

        Self {
            block,
            index,
            id: message.id().into_bytes(),
            source: message.source().into_bytes(),
            destination: message.destination().into_bytes(),
            payload: Bytes(message.payload_bytes().to_vec()),
            value: message.value().to_string(),
            reply,
            ack: None,
        }
    }

    fn acknowledgement() -> Self {
        Self {
            block: Hash::default(),
            index: 0,
            id: [0; 32],
            source: [0; 32],
            destination: [0; 32],
            payload: Vec::new().into(),
            value: "0".into(),
            reply: None,
            ack: Some(true),
        }
    }
}

#[rpc(server, namespace = "gear")]
pub trait GearEventsApi {
    #[subscription(
        name = "subscribeUserMessageSent",
        item = UserMessageSentJson,
        unsubscribe = "unsubscribeUserMessageSent"
    )]
    fn subscribe_user_message_sent(&self, filter: UserMsgFilter) -> SubscriptionResult;
}

pub struct GearEvents<C, B, Extractor>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
    Extractor: GearEventExtractor,
{
    dispatcher: Arc<Dispatcher<C, B, Extractor>>,
    executor: SubscriptionTaskExecutor,
}

impl<C, B, Extractor> GearEvents<C, B, Extractor>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
    Extractor: GearEventExtractor,
{
    pub fn new(client: Arc<C>, executor: SubscriptionTaskExecutor) -> Self {
        Self {
            dispatcher: Dispatcher::new(client, executor.clone()),
            executor,
        }
    }
}

#[async_trait]
impl<C, B, Extractor> GearEventsApiServer for GearEvents<C, B, Extractor>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
    Extractor: GearEventExtractor,
{
    fn subscribe_user_message_sent(
        &self,
        pending: PendingSubscriptionSink,
        filter: UserMsgFilter,
    ) -> SubscriptionResult {
        self.dispatcher.validate_filter(&filter)?;

        let plan = self.dispatcher.prepare_plan(&filter)?;
        let dispatcher = self.dispatcher.clone();
        let executor = self.executor.clone();

        let fut: BoxFuture<'static, ()> = Box::pin(async move {
            dispatcher.run_subscription(filter, plan, pending).await;
        });

        executor.spawn("gear-user-message-sent", None, fut);

        Ok(())
    }
}

struct Dispatcher<C, B, Extractor>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
    Extractor: GearEventExtractor,
{
    client: Arc<C>,
    events_key: StorageKey,
    inner: Mutex<Subscribers>,
    _marker: PhantomData<(B, Extractor)>,
}

impl<C, B, Extractor> Dispatcher<C, B, Extractor>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
    Extractor: GearEventExtractor,
{
    fn new(client: Arc<C>, executor: SubscriptionTaskExecutor) -> Arc<Self> {
        let dispatcher = Arc::new(Self {
            client: client.clone(),
            events_key: Extractor::events_storage_key(),
            inner: Mutex::new(Subscribers::default()),
            _marker: Default::default(),
        });

        dispatcher.spawn_import_stream(executor.clone());
        dispatcher.spawn_finality_stream(executor);

        dispatcher
    }

    fn validate_filter(&self, filter: &UserMsgFilter) -> Result<(), ErrorObjectOwned> {
        for (index, payload_filter) in filter.payload_filters.iter().enumerate() {
            if payload_filter.pattern.len() > MAX_PAYLOAD_PATTERN {
                return Err(invalid_params(format!(
                    "payload_filters[{index}] longer than {MAX_PAYLOAD_PATTERN} bytes"
                )));
            }
        }
        Ok(())
    }

    fn prepare_plan(&self, filter: &UserMsgFilter) -> Result<BackfillPlan, ErrorObjectOwned> {
        let info = self.client.info();
        let kind = if filter.finalized_only.unwrap_or(false) {
            StreamKind::Finalized
        } else {
            StreamKind::Best
        };

        let target = match kind {
            StreamKind::Best => info.best_number,
            StreamKind::Finalized => info.finalized_number,
        }
        .unique_saturated_into();

        let maybe_range = match filter.from_block {
            Some(start) if start <= target => {
                let span = target - start + 1;
                if span > MAX_BACKFILL_BLOCKS {
                    return Err(invalid_params(format!(
                        "requested backfill spans {span} blocks which exceeds limit {MAX_BACKFILL_BLOCKS}"
                    )));
                }

                Some(start..=target)
            }
            _ => None,
        };

        let skip_until = match (filter.from_block, &maybe_range) {
            (Some(_), Some(range)) => Some(*range.end()),
            (Some(start), None) => Some(start.saturating_sub(1)),
            _ => None,
        };

        Ok(BackfillPlan {
            kind,
            range: maybe_range,
            skip_until,
        })
    }

    async fn run_subscription(
        &self,
        filter: UserMsgFilter,
        plan: BackfillPlan,
        pending: PendingSubscriptionSink,
    ) {
        let sink = match pending.accept().await {
            Ok(sink) => sink,
            Err(error) => {
                debug!(target: "rpc", "failed to accept user message subscription: {error:?}");
                return;
            }
        };

        match SubscriptionMessage::from_json(&UserMessageSentJson::acknowledgement()) {
            Ok(initial) => {
                if sink.send(initial).await.is_err() {
                    return;
                }
            }
            Err(error) => {
                error!(
                    target: "rpc",
                    "unable to serialize initial user message subscription ack: {error}"
                );
            }
        }

        if let Err(error) = self.backfill(&sink, &filter, &plan).await {
            warn!(target: "rpc", "failed to backfill user message subscription: {error:?}");
            return;
        }

        let mut filter = filter;
        filter.from_block = None;

        let mut subscribers = self.inner.lock();
        let id = subscribers.next_id;
        subscribers.next_id += 1;
        let stream = subscribers.stream_mut(plan.kind);
        stream.subs.insert(
            id,
            Subscriber {
                sink,
                filter,
                skip_until: plan.skip_until,
            },
        );
    }

    async fn backfill(
        &self,
        sink: &SubscriptionSink,
        filter: &UserMsgFilter,
        plan: &BackfillPlan,
    ) -> Result<(), ErrorObjectOwned> {
        let Some(range) = plan.range.clone() else {
            return Ok(());
        };

        for number in range {
            let block_number: BlockNumber = number.saturated_into();
            let Some(hash) = (match self.client.hash(block_number) {
                Ok(opt) => opt,
                Err(error) => {
                    warn!(target: "rpc", "failed to resolve block hash during backfill: {error}");
                    continue;
                }
            }) else {
                continue;
            };

            let events = match self.load_events(hash) {
                Ok(events) => events,
                Err(error) => {
                    warn!(target: "rpc", "failed to decode events during backfill: {error}");
                    continue;
                }
            };

            for (index, record) in events.into_iter().enumerate() {
                let Some(message) = Extractor::extract_user_message(&record.event) else {
                    continue;
                };

                if !matches_filter(filter, &message) {
                    continue;
                }

                let payload = UserMessageSentJson::from_message(hash, index as u32, &message);
                let msg = SubscriptionMessage::from_json(&payload)
                    .map_err(|error| internal_error(error.to_string()))?;

                if let Err(error) = sink.send(msg).await {
                    match error {
                        DisconnectError(_) => {
                            debug!(target: "rpc", "subscription backfill aborted because client disconnected");
                        }
                    }
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    fn spawn_import_stream(self: &Arc<Self>, executor: SubscriptionTaskExecutor) {
        let dispatcher = Arc::clone(self);

        let fut: BoxFuture<'static, ()> = Box::pin(async move {
            let mut stream = dispatcher.client.import_notification_stream();
            while let Some(notification) = stream.next().await {
                if !notification.is_new_best {
                    continue;
                }
                let number = (*notification.header.number()).unique_saturated_into();
                dispatcher.process_block(StreamKind::Best, notification.hash, number);
            }
        });

        executor.spawn("gear-user-message-head-stream", None, fut);
    }

    fn spawn_finality_stream(self: &Arc<Self>, executor: SubscriptionTaskExecutor) {
        let dispatcher = Arc::clone(self);

        let fut: BoxFuture<'static, ()> = Box::pin(async move {
            let mut stream = dispatcher.client.finality_notification_stream();
            while let Some(notification) = stream.next().await {
                let number = (*notification.header.number()).unique_saturated_into();
                dispatcher.process_block(StreamKind::Finalized, notification.hash, number);
            }
        });

        executor.spawn("gear-user-message-finalized-stream", None, fut);
    }

    fn process_block(&self, kind: StreamKind, hash: Hash, number: u64) {
        if !self.has_subscribers(kind) {
            return;
        }

        let events = match self.load_events(hash) {
            Ok(events) => events,
            Err(error) => {
                warn!(target: "rpc", "failed to decode events for block {hash:?}: {error}");
                return;
            }
        };

        if events.is_empty() {
            return;
        }

        let mut subscribers = self.inner.lock();
        let stream = subscribers.stream_mut(kind);
        if stream.subs.is_empty() {
            return;
        }

        for (index, record) in events.into_iter().enumerate() {
            let Some(message) = Extractor::extract_user_message(&record.event) else {
                continue;
            };

            let payload = UserMessageSentJson::from_message(hash, index as u32, &message);
            let msg = match SubscriptionMessage::from_json(&payload) {
                Ok(msg) => msg,
                Err(error) => {
                    error!(
                        target: "rpc",
                        "unable to serialize UserMessageSent notification for block {hash:?}: {error}"
                    );
                    continue;
                }
            };

            let mut dead = Vec::new();

            for (id, subscriber) in stream.subs.iter_mut() {
                if let Some(skip) = subscriber.skip_until {
                    if number <= skip {
                        continue;
                    }
                    subscriber.skip_until = None;
                }

                if !matches_filter(&subscriber.filter, &message) {
                    continue;
                }

                if let Err(error) = subscriber.sink.try_send(msg.clone()) {
                    match error {
                        TrySendError::Full(_) => {
                            warn!(
                                target: "rpc",
                                "dropping user message subscriber because client is too slow"
                            );
                        }
                        TrySendError::Closed(_) => {
                            debug!(target: "rpc", "user message subscriber disconnected");
                        }
                    }
                    dead.push(*id);
                }
            }

            for id in dead {
                stream.subs.remove(&id);
            }
        }
    }

    fn has_subscribers(&self, kind: StreamKind) -> bool {
        let subscribers = self.inner.lock();
        !subscribers.stream(kind).subs.is_empty()
    }

    fn load_events(
        &self,
        hash: Hash,
    ) -> Result<Vec<frame_system::EventRecord<Extractor::RuntimeEvent, Hash>>, DecodeError> {
        let maybe_raw = self
            .client
            .storage(hash, &self.events_key)
            .map_err(|error| DecodeError::Storage(error.to_string()))?;

        let StorageData(raw) = match maybe_raw {
            Some(data) => data,
            None => return Ok(Vec::new()),
        };

        let events =
            Decode::decode(&mut &raw[..]).map_err(|error| DecodeError::Codec(error.to_string()))?;

        Ok(events)
    }
}

#[derive(Clone)]
struct BackfillPlan {
    kind: StreamKind,
    range: Option<RangeInclusive<u64>>,
    skip_until: Option<u64>,
}

#[derive(Default)]
struct Subscribers {
    next_id: u64,
    best: StreamState,
    finalized: StreamState,
}

impl Subscribers {
    fn stream(&self, kind: StreamKind) -> &StreamState {
        match kind {
            StreamKind::Best => &self.best,
            StreamKind::Finalized => &self.finalized,
        }
    }

    fn stream_mut(&mut self, kind: StreamKind) -> &mut StreamState {
        match kind {
            StreamKind::Best => &mut self.best,
            StreamKind::Finalized => &mut self.finalized,
        }
    }
}

#[derive(Default)]
struct StreamState {
    subs: HashMap<u64, Subscriber>,
}

struct Subscriber {
    sink: SubscriptionSink,
    filter: UserMsgFilter,
    skip_until: Option<u64>,
}

#[derive(Clone, Copy)]
enum StreamKind {
    Best,
    Finalized,
}

pub(crate) trait GearEventExtractor: Send + Sync + 'static {
    type RuntimeEvent: Parameter + Send + Sync + 'static;

    fn events_storage_key() -> StorageKey;
    fn extract_user_message(event: &Self::RuntimeEvent) -> Option<UserMessage>;
}

fn matches_filter(filter: &UserMsgFilter, message: &UserMessage) -> bool {
    if let Some(source) = filter.source
        && message.source().into_bytes() != source.0
    {
        return false;
    }

    if let Some(destination) = filter.destination
        && message.destination().into_bytes() != destination.0
    {
        return false;
    }

    let payload = message.payload_bytes();

    for payload_filter in &filter.payload_filters {
        let offset = payload_filter.offset as usize;
        let pattern = &payload_filter.pattern;

        let Some(end) = offset.checked_add(pattern.len()) else {
            return false;
        };

        if end > payload.len() {
            return false;
        }

        if &payload[offset..end] != pattern.0.as_slice() {
            return false;
        }
    }

    true
}

fn invalid_params(message: impl Into<String>) -> ErrorObjectOwned {
    jsonrpsee::types::error::ErrorObject::owned(
        jsonrpsee::types::error::ErrorCode::InvalidParams.code(),
        message,
        None::<()>,
    )
}

fn internal_error(message: impl Into<String>) -> ErrorObjectOwned {
    jsonrpsee::types::error::ErrorObject::owned(
        jsonrpsee::types::error::ErrorCode::InternalError.code(),
        message,
        None::<()>,
    )
}

enum DecodeError {
    Storage(String),
    Codec(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Storage(message) => write!(f, "storage error: {message}"),
            DecodeError::Codec(message) => write!(f, "codec error: {message}"),
        }
    }
}

impl std::fmt::Debug for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for DecodeError {}

#[cfg(feature = "vara-native")]
pub type VaraGearEvents<C, B> = GearEvents<C, B, VaraExtractor>;

#[cfg(feature = "vara-native")]
pub struct VaraExtractor;

#[cfg(feature = "vara-native")]
impl GearEventExtractor for VaraExtractor {
    type RuntimeEvent = crate::vara_runtime::RuntimeEvent;

    fn events_storage_key() -> StorageKey {
        let key = storage_prefix(b"System", b"Events");
        StorageKey(key.to_vec())
    }

    fn extract_user_message(event: &Self::RuntimeEvent) -> Option<UserMessage> {
        match event {
            crate::vara_runtime::RuntimeEvent::Gear(pallet_gear::Event::UserMessageSent {
                message,
                ..
            }) => Some(message.clone()),
            _ => None,
        }
    }
}

#[cfg(feature = "vara-native")]
pub fn create_vara_events<C, B>(
    client: Arc<C>,
    executor: SubscriptionTaskExecutor,
) -> VaraGearEvents<C, B>
where
    C: BlockchainEvents<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, B>
        + Send
        + Sync
        + 'static,
    B: ClientBackend<Block> + Send + Sync + 'static,
{
    GearEvents::new(client, executor)
}

#[cfg(test)]
mod tests {
    use super::{PayloadFilter, UserMsgFilter, matches_filter};
    use core::convert::TryFrom;
    use gear_core::{
        buffer::Payload,
        ids::{ActorId, MessageId},
        message::UserMessage,
    };

    fn user_message_with_payload(payload: &[u8]) -> UserMessage {
        UserMessage::new(
            MessageId::default(),
            ActorId::default(),
            ActorId::from(1u64),
            Payload::try_from(payload.to_vec()).expect("payload within bounds"),
            0,
            None,
        )
    }

    #[test]
    fn payload_filter_matches_at_offset() {
        let filter = UserMsgFilter {
            source: None,
            destination: None,
            payload_filters: vec![PayloadFilter {
                offset: 2,
                pattern: b"cd".to_vec(),
            }],
            from_block: None,
            finalized_only: None,
        };

        let message = user_message_with_payload(b"abcdef");
        assert!(matches_filter(&filter, &message));
    }

    #[test]
    fn payload_filter_rejects_out_of_bounds() {
        let filter = UserMsgFilter {
            source: None,
            destination: None,
            payload_filters: vec![PayloadFilter {
                offset: 5,
                pattern: b"ghi".to_vec(),
            }],
            from_block: None,
            finalized_only: None,
        };

        let message = user_message_with_payload(b"abcdef");
        assert!(!matches_filter(&filter, &message));
    }

    #[test]
    fn multiple_payload_filters_must_all_match() {
        let filter = UserMsgFilter {
            source: None,
            destination: None,
            payload_filters: vec![
                PayloadFilter {
                    offset: 3,
                    pattern: b"de".to_vec(),
                },
                PayloadFilter {
                    offset: 0,
                    pattern: b"ab".to_vec(),
                },
            ],
            from_block: None,
            finalized_only: None,
        };

        let message = user_message_with_payload(b"abcdef");
        assert!(matches_filter(&filter, &message));

        let mismatched_filter = user_message_with_payload(b"abcxef");
        assert!(!matches_filter(&filter, &mismatched_filter));
    }
}
