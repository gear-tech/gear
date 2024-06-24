use alloy::{
    consensus::{SidecarCoder, SimpleCoder},
    eips::eip4844::kzg_to_versioned_hash,
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::PubSubFrontend,
    rpc::{
        client::WsConnect,
        types::{
            beacon::sidecar::BeaconBlobBundle,
            eth::{BlockTransactionsKind, Filter, Topic},
        },
    },
};
use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, Stream, StreamExt};
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
use hypercore_ethereum::event::{
    ClaimValue, CodeApproved, CodeRejected, CreateProgram, SendMessage, SendReply, UpdatedProgram,
    UploadCode, UserMessageSent, UserReplySent,
};
use reqwest::Client;
use std::{collections::HashSet, hash::RandomState};
use tokio::time::{self, Duration};

use crate::{event::BlockEventData, BlockEvent, Event};

#[derive(Debug)]
pub(crate) struct PendingUploadCode {
    origin: ActorId,
    code_id: CodeId,
    blob_tx: H256,
    tx_hash: H256,
}

impl PendingUploadCode {
    fn blob_tx(&self) -> H256 {
        if self.blob_tx.is_zero() {
            self.tx_hash
        } else {
            self.blob_tx
        }
    }
}

pub(crate) type ObserverProvider = RootProvider<PubSubFrontend>;

pub struct Observer {
    provider: ObserverProvider,
    ethereum_beacon_rpc: String,
    http_client: Client,
    router_address: Address,
}

impl Observer {
    const ROUTER_EVENT_SIGNATURE_HASHES: [B256; 10] = [
        B256::new(UploadCode::SIGNATURE_HASH),
        B256::new(CodeApproved::SIGNATURE_HASH),
        B256::new(CodeRejected::SIGNATURE_HASH),
        B256::new(CreateProgram::SIGNATURE_HASH),
        B256::new(UpdatedProgram::SIGNATURE_HASH),
        B256::new(UserMessageSent::SIGNATURE_HASH),
        B256::new(UserReplySent::SIGNATURE_HASH),
        B256::new(SendMessage::SIGNATURE_HASH),
        B256::new(SendReply::SIGNATURE_HASH),
        B256::new(ClaimValue::SIGNATURE_HASH),
    ];

    pub async fn new(
        ethereum_rpc: String,
        ethereum_beacon_rpc: String,
        router_address: String,
    ) -> Result<Self> {
        Ok(Self {
            provider: ProviderBuilder::new()
                .on_ws(WsConnect::new(ethereum_rpc))
                .await?,
            ethereum_beacon_rpc,
            http_client: Client::new(),
            router_address: Address::parse_checksummed(router_address, None)?,
        })
    }

    pub fn events(&mut self) -> impl Stream<Item = Event> + '_ {
        async_stream::stream! {
            let block_subscription = self
                .provider
                .subscribe_blocks()
                .await
                .expect("failed to subscribe to blocks");
            let mut block_stream = block_subscription.into_stream();
            let mut futures = FuturesUnordered::new();

            loop {
                tokio::select! {
                    block = block_stream.next() => {
                        match block {
                            Some(block) => {
                                let block_header = block.header;
                                let block_hash = block_header.hash.expect("failed to get block hash");
                                let parent_hash = block_header.parent_hash;
                                let block_number = block_header.number.expect("failed to get block number");
                                let timestamp = block_header.timestamp;
                                log::debug!("block {block_number}, hash {block_hash}, parent hash: {parent_hash}");

                                match read_block_events(H256(block_hash.0), &mut self.provider, self.router_address).await {
                                    Ok((pending_upload_codes, events)) => {
                                        for pending_upload_code in pending_upload_codes {
                                            let provider = self.provider.clone();
                                            let http_client = self.http_client.clone();
                                            let beacon_rpc_url = self.ethereum_beacon_rpc.clone();
                                            let origin = pending_upload_code.origin;
                                            let tx_hash = pending_upload_code.blob_tx();
                                            let attempts = Some(3);
                                            let code_id = pending_upload_code.code_id;

                                            futures.push(async move {
                                                Self::read_code_from_tx_hash(
                                                    provider,
                                                    http_client,
                                                    &beacon_rpc_url,
                                                    origin,
                                                    tx_hash,
                                                    attempts,
                                                    code_id,
                                                ).await
                                            });
                                        }

                                        let block_data = BlockEventData {
                                            block_hash: H256(block_hash.0),
                                            parent_hash: H256(parent_hash.0),
                                            block_number,
                                            block_timestamp: timestamp,
                                            events,
                                        };

                                        yield Event::Block(block_data);
                                    }
                                    Err(err) => log::error!("failed to read events: {err}"),
                                }
                            },
                            None => break,
                        }
                    }
                    future = futures.next(), if !futures.is_empty() => {
                        match future {
                            Some(future) => {
                                match future {
                                    Ok((origin, code_id, code)) => {
                                        yield Event::UploadCode { origin, code_id, code };
                                    },
                                    Err(err) => log::error!("failed to handle upload code event: {err}"),
                                }
                            },
                            None => continue,
                        }
                    }
                };
            }
        }
    }

    async fn read_code_from_tx_hash(
        provider: ObserverProvider,
        http_client: Client,
        beacon_rpc_url: &str,
        origin: ActorId,
        tx_hash: H256,
        attempts: Option<u8>,
        expected_code_id: CodeId,
    ) -> Result<(ActorId, CodeId, Vec<u8>)> {
        let code =
            Self::read_blob_from_tx_hash(provider, http_client, beacon_rpc_url, tx_hash, attempts)
                .await
                .map_err(|err| anyhow!("failed to read blob: {err}"))?;

        (CodeId::generate(&code) == expected_code_id)
            .then_some(())
            .ok_or_else(|| anyhow!("unexpected code id"))?;

        Ok((origin, expected_code_id, code))
    }

    async fn read_blob_from_tx_hash(
        provider: ObserverProvider,
        http_client: Client,
        beacon_rpc_url: &str,
        tx_hash: H256,
        attempts: Option<u8>,
    ) -> Result<Vec<u8>> {
        //TODO: read genesis from `{beacon_rpc_url}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1695902400;
        const BEACON_BLOCK_TIME: u64 = 12;

        let tx = provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or_else(|| anyhow!("failed to get transaction"))?;
        let blob_versioned_hashes = tx
            .blob_versioned_hashes
            .ok_or_else(|| anyhow!("failed to get versioned hashes"))?;
        let blob_versioned_hashes = HashSet::<_, RandomState>::from_iter(blob_versioned_hashes);
        let block_hash = tx
            .block_hash
            .ok_or_else(|| anyhow!("failed to get block hash"))?;
        let block = provider
            .get_block_by_hash(block_hash, BlockTransactionsKind::Hashes)
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

pub(crate) async fn read_block_events(
    block_hash: H256,
    provider: &mut ObserverProvider,
    router_address: Address,
) -> Result<(Vec<PendingUploadCode>, Vec<BlockEvent>)> {
    let router_events_filter = Filter::new()
        .at_block_hash(block_hash.0)
        .address(router_address)
        .event_signature(Topic::from_iter(Observer::ROUTER_EVENT_SIGNATURE_HASHES));

    let logs = provider.get_logs(&router_events_filter).await?;

    let mut pending_upload_codes = vec![];
    let block_events: Vec<_> = logs
        .into_iter()
        .filter_map(|ref log| match log.topic0().copied().map(|bytes| bytes.0) {
            Some(UploadCode::SIGNATURE_HASH) => {
                let UploadCode {
                    origin,
                    code_id,
                    blob_tx,
                } = log.try_into().ok()?;

                let tx_hash = H256(log.transaction_hash?.0);

                pending_upload_codes.push(PendingUploadCode {
                    origin,
                    code_id,
                    blob_tx,
                    tx_hash,
                });

                None
            }
            Some(CodeApproved::SIGNATURE_HASH) => {
                Some(BlockEvent::CodeApproved(log.try_into().ok()?))
            }
            Some(CodeRejected::SIGNATURE_HASH) => {
                Some(BlockEvent::CodeRejected(log.try_into().ok()?))
            }
            Some(CreateProgram::SIGNATURE_HASH) => {
                Some(BlockEvent::CreateProgram(log.try_into().ok()?))
            }
            Some(UpdatedProgram::SIGNATURE_HASH) => {
                Some(BlockEvent::UpdatedProgram(log.try_into().ok()?))
            }
            Some(UserMessageSent::SIGNATURE_HASH) => {
                Some(BlockEvent::UserMessageSent(log.try_into().ok()?))
            }
            Some(UserReplySent::SIGNATURE_HASH) => {
                Some(BlockEvent::UserReplySent(log.try_into().ok()?))
            }
            Some(SendMessage::SIGNATURE_HASH) => {
                Some(BlockEvent::SendMessage(log.try_into().ok()?))
            }
            Some(SendReply::SIGNATURE_HASH) => Some(BlockEvent::SendReply(log.try_into().ok()?)),
            Some(ClaimValue::SIGNATURE_HASH) => Some(BlockEvent::ClaimValue(log.try_into().ok()?)),
            _ => None,
        })
        .collect();

    Ok((pending_upload_codes, block_events))
}
