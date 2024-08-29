use crate::observer::ObserverProvider;
use alloy::{
    consensus::{SidecarCoder, SimpleCoder},
    eips::eip4844::kzg_to_versioned_hash,
    providers::{Provider, ProviderBuilder},
    rpc::types::{beacon::sidecar::BeaconBlobBundle, eth::BlockTransactionsKind},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use gprimitives::H256;
use reqwest::Client;
use std::{
    collections::{HashMap, HashSet},
    hash::RandomState,
    sync::Arc,
};
use tokio::{
    sync::RwLock,
    time::{self, Duration},
};

#[async_trait]
pub trait BlobReader: Send + Sync {
    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>>;
}

#[derive(Clone)]
pub struct ConsensusLayerBlobReader {
    provider: ObserverProvider,
    http_client: Client,
    ethereum_beacon_rpc: String,
    beacon_block_time: Duration,
}

impl ConsensusLayerBlobReader {
    pub async fn new(
        ethereum_rpc: &str,
        ethereum_beacon_rpc: &str,
        beacon_block_time: Duration,
    ) -> Result<Self> {
        Ok(Self {
            provider: ProviderBuilder::new().on_builtin(ethereum_rpc).await?,
            http_client: Client::new(),
            ethereum_beacon_rpc: ethereum_beacon_rpc.into(),
            beacon_block_time,
        })
    }

    async fn read_blob_bundle(&self, slot: u64) -> reqwest::Result<BeaconBlobBundle> {
        let ethereum_beacon_rpc = &self.ethereum_beacon_rpc;
        self.http_client
            .get(format!(
                "{ethereum_beacon_rpc}/eth/v1/beacon/blob_sidecars/{slot}"
            ))
            .send()
            .await?
            .json::<BeaconBlobBundle>()
            .await
    }
}

#[async_trait]
impl BlobReader for ConsensusLayerBlobReader {
    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        //TODO: read genesis from `{ethereum_beacon_rpc}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1695902400;

        let tx = self
            .provider
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
        let block = self
            .provider
            .get_block_by_hash(block_hash, BlockTransactionsKind::Hashes)
            .await?
            .ok_or_else(|| anyhow!("failed to get block"))?;
        let slot =
            (block.header.timestamp - BEACON_GENESIS_BLOCK_TIME) / self.beacon_block_time.as_secs();
        let blob_bundle_result = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::debug!("trying to get blob, attempt #{}", count + 1);
                    let blob_bundle_result = self.read_blob_bundle(slot).await;
                    if blob_bundle_result.is_ok() || count >= attempts {
                        break blob_bundle_result;
                    } else {
                        time::sleep(self.beacon_block_time).await;
                        count += 1;
                    }
                }
            }
            None => self.read_blob_bundle(slot).await,
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
            .ok_or_else(|| anyhow!("failed to decode blobs"))?
            .concat();

        Ok(data)
    }
}

#[derive(Clone)]
pub struct MockBlobReader {
    transactions: Arc<RwLock<HashMap<H256, Vec<u8>>>>,
    block_time: Duration,
}

impl MockBlobReader {
    pub fn new(block_time: Duration) -> Self {
        Self {
            transactions: Arc::new(RwLock::new(HashMap::new())),
            block_time,
        }
    }

    pub async fn add_blob_transaction(&self, tx_hash: H256, data: Vec<u8>) {
        self.transactions.write().await.insert(tx_hash, data);
    }
}

#[async_trait]
impl BlobReader for MockBlobReader {
    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        let maybe_blob_data = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::debug!("trying to get blob, attempt #{}", count + 1);
                    let maybe_blob_data = self.transactions.read().await.get(&tx_hash).cloned();
                    if maybe_blob_data.is_some() || count >= attempts {
                        break maybe_blob_data;
                    } else {
                        time::sleep(self.block_time).await;
                        count += 1;
                    }
                }
            }
            None => self.transactions.read().await.get(&tx_hash).cloned(),
        };
        let blob_data = maybe_blob_data.ok_or_else(|| anyhow!("failed to get blob"))?;
        Ok(blob_data)
    }
}
