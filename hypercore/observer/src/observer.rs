use crate::{consts::*, event::*, Program, Router};
use alloy::{
    consensus::{SidecarCoder, SimpleCoder},
    eips::eip4844::kzg_to_versioned_hash,
    primitives::{Address, FixedBytes, LogData, TxHash, B256},
    providers::Provider,
    rpc::types::{
        beacon::sidecar::BeaconBlobBundle,
        eth::{Filter, Log, Topic},
    },
    sol_types::{self, SolEvent},
    transports::Transport,
};
use anyhow::{anyhow, Result};
use futures::{Stream, StreamExt};
use gear_core::ids::{prelude::*, CodeId};
use reqwest::Client;
use std::{collections::HashSet, hash::RandomState, marker::PhantomData};
use tokio::time::{self, Duration};

pub struct Observer<T, P> {
    provider: P,
    router_address: Address,
    phantom: PhantomData<T>,
}

impl<T: Transport + Clone, P: Provider<T> + Clone + 'static> Observer<T, P> {
    const ROUTER_EVENT_SIGNATURE_HASHES: [B256; 2] = [
        <Router::UploadCode as SolEvent>::SIGNATURE_HASH,
        <Router::CreateProgram as SolEvent>::SIGNATURE_HASH,
    ];
    const PROGRAM_EVENT_SIGNATURE_HASHES: [B256; 3] = [
        <Program::SendMessage as SolEvent>::SIGNATURE_HASH,
        <Program::SendReply as SolEvent>::SIGNATURE_HASH,
        <Program::ClaimValue as SolEvent>::SIGNATURE_HASH,
    ];

    pub fn new(provider: P, router_address: Address) -> Self {
        Self {
            provider,
            router_address,
            phantom: PhantomData,
        }
    }

    pub fn listen(mut self) -> impl Stream<Item = Result<Vec<(Event, Log)>>> {
        async_stream::try_stream! {
            let block_subscription = self.provider.subscribe_blocks().await?;
            let mut block_stream = block_subscription.into_stream();

            loop {
                if let Some(block) = block_stream.next().await {
                    let block_header = block.header;
                    let block_number = block_header
                        .number
                        .ok_or_else(|| anyhow!("failed to get block number"))?;
                    let block_hash = block_header
                        .hash
                        .ok_or_else(|| anyhow!("failed to get block hash"))?;
                    log::debug!("block {block_number}, hash {block_hash}");

                    let events_result = self.read_events(block_hash).await;
                    if let Err(ref err) = events_result {
                        log::error!("failed to handle events: {err}")
                    }

                    if let Some(events) = events_result? {
                        yield events;
                    }
                }
            }
        }
    }

    async fn read_events(&mut self, block_hash: B256) -> Result<Option<Vec<(Event, Log)>>> {
        let [router_filter, program_filter] = self.event_filters(block_hash);

        let mut logs = self.provider.get_logs(&router_filter).await?;
        let mut logs1 = self.provider.get_logs(&program_filter).await?;
        logs.append(&mut logs1);

        if logs.is_empty() {
            return Ok(None);
        }

        logs.sort_unstable_by_key(|log| (log.block_timestamp, log.log_index));

        let logs: Vec<_> = logs
            .into_iter()
            .filter_map(|log| match log.topic0().copied() {
                Some(<Router::UploadCode as SolEvent>::SIGNATURE_HASH) => Some((
                    Event::UploadCode(Self::decode_log::<Router::UploadCode>(&log).ok()?),
                    log,
                )),
                Some(<Router::CreateProgram as SolEvent>::SIGNATURE_HASH) => Some((
                    Event::CreateProgram(Self::decode_log::<Router::CreateProgram>(&log).ok()?),
                    log,
                )),
                Some(<Program::SendMessage as SolEvent>::SIGNATURE_HASH) => Some((
                    Event::SendMessage(Self::decode_log::<Program::SendMessage>(&log).ok()?),
                    log,
                )),
                Some(<Program::SendReply as SolEvent>::SIGNATURE_HASH) => Some((
                    Event::SendReply(Self::decode_log::<Program::SendReply>(&log).ok()?),
                    log,
                )),
                Some(<Program::ClaimValue as SolEvent>::SIGNATURE_HASH) => Some((
                    Event::ClaimValue(Self::decode_log::<Program::ClaimValue>(&log).ok()?),
                    log,
                )),
                _ => None,
            })
            .collect();

        Ok(if logs.is_empty() { None } else { Some(logs) })
    }

    fn event_filters(&self, block_hash: B256) -> [Filter; 2] {
        [
            Filter::new()
                .at_block_hash(block_hash)
                .address(self.router_address)
                .event_signature(Topic::from_iter(Self::ROUTER_EVENT_SIGNATURE_HASHES)),
            Filter::new()
                .at_block_hash(block_hash)
                .event_signature(Topic::from_iter(Self::PROGRAM_EVENT_SIGNATURE_HASHES)),
        ]
    }

    fn decode_log<E: SolEvent>(log: &Log) -> sol_types::Result<E> {
        let log_data: &LogData = log.as_ref();
        E::decode_raw_log(log_data.topics().iter().copied(), &log_data.data, false)
    }

    pub async fn read_code_from_tx_hash(
        provider: P,
        http_client: Client,
        tx_hash: TxHash,
        attempts: Option<u8>,
        expected_code_id: FixedBytes<32>,
    ) -> Result<Vec<u8>> {
        let code = Self::read_blob_from_tx_hash(provider, http_client, tx_hash, attempts)
            .await
            .map_err(|err| anyhow!("failed to read blob: {err}"))?;

        (CodeId::generate(&code).into_bytes() == expected_code_id)
            .then_some(())
            .ok_or_else(|| anyhow!("unexpected code id"))?;

        Ok(code)
    }

    async fn read_blob_from_tx_hash(
        provider: P,
        http_client: Client,
        tx_hash: TxHash,
        attempts: Option<u8>,
    ) -> Result<Vec<u8>> {
        let tx = provider
            .get_transaction_by_hash(tx_hash)
            .await?
            .ok_or_else(|| anyhow!("failed to get transaction"))?;
        let blob_versioned_hashes = tx
            .blob_versioned_hashes
            .ok_or_else(|| anyhow!("failed to get versioned hashes"))?;
        let blob_versioned_hashes = HashSet::<_, RandomState>::from_iter(blob_versioned_hashes);
        let block_number = tx
            .block_number
            .ok_or_else(|| anyhow!("failed to get block number"))?;
        let block = provider
            .get_block_by_number(block_number.into(), false)
            .await?
            .ok_or_else(|| anyhow!("failed to get block"))?;
        let slot = (block.header.timestamp - BEACON_GENESIS_BLOCK_TIME) / BEACON_BLOCK_TIME;
        let blob_bundle_result = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::debug!("trying to get blob, attempt #{}", count + 1);
                    let blob_bundle_result =
                        Self::read_blob_bundle(http_client.clone(), slot).await;
                    if blob_bundle_result.is_ok() || count >= attempts {
                        break blob_bundle_result;
                    } else {
                        time::sleep(Duration::from_secs(BEACON_BLOCK_TIME)).await;
                        count += 1;
                    }
                }
            }
            None => Self::read_blob_bundle(http_client, slot).await,
        };
        let blob_bundle = blob_bundle_result?;

        let mut blobs = Vec::with_capacity(blob_versioned_hashes.len());
        for blob_data in blob_bundle.into_iter().filter(|blob_data| {
            blob_versioned_hashes
                .contains(&kzg_to_versioned_hash(blob_data.kzg_commitment.as_ref()))
        }) {
            blobs.push(*blob_data.blob);
        }

        let mut coder = SimpleCoder::default();
        let data = coder
            .decode_all(&blobs)
            .ok_or(anyhow!("failed to decode blobs"))?
            .concat();

        Ok(data)
    }

    async fn read_blob_bundle(http_client: Client, slot: u64) -> reqwest::Result<BeaconBlobBundle> {
        http_client
            .get(format!(
                "{BEACON_RPC_URL}/eth/v1/beacon/blob_sidecars/{slot}"
            ))
            .send()
            .await?
            .json::<BeaconBlobBundle>()
            .await
    }
}
