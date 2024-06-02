use crate::{BlockEvent, Event, Program, Router};
use alloy::{
    consensus::{SidecarCoder, SimpleCoder},
    eips::eip4844::kzg_to_versioned_hash,
    primitives::{Address, LogData, TxHash, B256},
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
use gear_core::ids::{prelude::*, ActorId, CodeId, MessageId};
use gprimitives::H256;
use reqwest::Client;
use std::{collections::HashSet, hash::RandomState, marker::PhantomData};
use tokio::time::{self, Duration};

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct PendingUploadCode {
    origin: ActorId,
    code_id: CodeId,
    blob_tx: H256,
    tx_hash: TxHash,
}

pub struct Observer<T, P> {
    provider: P,
    router_address: Address,
    pending_upload_codes: HashSet<PendingUploadCode>,
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
            pending_upload_codes: HashSet::new(),
            router_address,
            phantom: PhantomData,
        }
    }

    pub fn events(&mut self) -> impl Stream<Item = Event> + '_ {
        async_stream::stream! {
            let block_subscription = self
                .provider
                .subscribe_blocks()
                .await
                .expect("failed to subscribe to blocks");
            let mut block_stream = block_subscription.into_stream();

            while let Some(block) = block_stream.next().await {
                let block_header = block.header;
                let block_number = block_header.number.expect("failed to get block number");
                let block_hash = block_header.hash.expect("failed to get block hash");
                log::debug!("block {block_number}, hash {block_hash}");

                match self.read_events(block_hash).await {
                    Ok(events) => {
                        let block_hash = H256(block_hash.0);
                        yield Event::Block { block_hash, events };
                    }
                    Err(err) => log::error!("failed to read events: {err}"),
                }
            }
        }
    }

    async fn read_events(&mut self, block_hash: B256) -> Result<Vec<BlockEvent>> {
        let [router_filter, program_filter] = self.event_filters(block_hash);

        let mut logs = self.provider.get_logs(&router_filter).await?;
        let mut logs1 = self.provider.get_logs(&program_filter).await?;
        logs.append(&mut logs1);
        logs.sort_unstable_by_key(|log| (log.block_timestamp, log.log_index));

        let logs: Vec<_> = logs
            .into_iter()
            .filter_map(|log| match log.topic0().copied() {
                Some(<Router::UploadCode as SolEvent>::SIGNATURE_HASH) => {
                    let event = Self::decode_log::<Router::UploadCode>(&log).ok()?;

                    let origin = ActorId::new(event.origin.into_word().0);
                    let code_id = CodeId::new(event.codeId.0);
                    let blob_tx = H256(event.blobTx.0);
                    let tx_hash = log.transaction_hash?;

                    self.pending_upload_codes.insert(PendingUploadCode {
                        origin,
                        code_id,
                        blob_tx,
                        tx_hash,
                    });

                    None
                }
                Some(<Router::CreateProgram as SolEvent>::SIGNATURE_HASH) => {
                    let event = Self::decode_log::<Router::CreateProgram>(&log).ok()?;

                    let origin = ActorId::new(event.origin.into_word().0);
                    let code_id = CodeId::new(event.codeId.0);
                    let salt = event.salt.to_vec();
                    let init_payload = event.initPayload.to_vec();
                    let gas_limit = event.gasLimit;
                    let value = event.value;

                    Some(BlockEvent::CreateProgram {
                        origin,
                        code_id,
                        salt,
                        init_payload,
                        gas_limit,
                        value,
                    })
                }
                Some(<Program::SendMessage as SolEvent>::SIGNATURE_HASH) => {
                    let event = Self::decode_log::<Program::SendMessage>(&log).ok()?;

                    let origin = ActorId::new(event.origin.into_word().0);
                    let destination = ActorId::new(event.destination.into_word().0);
                    let payload = event.payload.to_vec();
                    let gas_limit = event.gasLimit;
                    let value = event.value;

                    Some(BlockEvent::SendMessage {
                        origin,
                        destination,
                        payload,
                        gas_limit,
                        value,
                    })
                }
                Some(<Program::SendReply as SolEvent>::SIGNATURE_HASH) => {
                    let event = Self::decode_log::<Program::SendReply>(&log).ok()?;

                    let origin = ActorId::new(event.origin.into_word().0);
                    let reply_to_id = MessageId::new(event.replyToId.0);
                    let payload = event.payload.to_vec();
                    let gas_limit = event.gasLimit;
                    let value = event.value;

                    Some(BlockEvent::SendReply {
                        origin,
                        reply_to_id,
                        payload,
                        gas_limit,
                        value,
                    })
                }
                Some(<Program::ClaimValue as SolEvent>::SIGNATURE_HASH) => {
                    let event = Self::decode_log::<Program::ClaimValue>(&log).ok()?;

                    let origin = ActorId::new(event.origin.into_word().0);
                    let message_id = MessageId::new(event.messageId.0);

                    Some(BlockEvent::ClaimValue { origin, message_id })
                }
                _ => None,
            })
            .collect();

        Ok(logs)
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

    async fn read_code_from_tx_hash(
        provider: P,
        http_client: Client,
        beacon_rpc_url: &str,
        tx_hash: TxHash,
        attempts: Option<u8>,
        expected_code_id: CodeId,
    ) -> Result<Vec<u8>> {
        let code =
            Self::read_blob_from_tx_hash(provider, http_client, beacon_rpc_url, tx_hash, attempts)
                .await
                .map_err(|err| anyhow!("failed to read blob: {err}"))?;

        (CodeId::generate(&code) == expected_code_id)
            .then_some(())
            .ok_or_else(|| anyhow!("unexpected code id"))?;

        Ok(code)
    }

    async fn read_blob_from_tx_hash(
        provider: P,
        http_client: Client,
        beacon_rpc_url: &str,
        tx_hash: TxHash,
        attempts: Option<u8>,
    ) -> Result<Vec<u8>> {
        //TODO: read genesis from `{beacon_rpc_url}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1695902400;
        const BEACON_BLOCK_TIME: u64 = 12;

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
                        Self::read_blob_bundle(http_client.clone(), beacon_rpc_url, slot).await;
                    if blob_bundle_result.is_ok() || count >= attempts {
                        break blob_bundle_result;
                    } else {
                        time::sleep(Duration::from_secs(BEACON_BLOCK_TIME)).await;
                        count += 1;
                    }
                }
            }
            None => Self::read_blob_bundle(http_client, beacon_rpc_url, slot).await,
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

    async fn read_blob_bundle(
        http_client: Client,
        beacon_rpc_url: &str,
        slot: u64,
    ) -> reqwest::Result<BeaconBlobBundle> {
        http_client
            .get(format!(
                "{beacon_rpc_url}/eth/v1/beacon/blob_sidecars/{slot}"
            ))
            .send()
            .await?
            .json::<BeaconBlobBundle>()
            .await
    }
}
