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

use alloy::{
    consensus::{SidecarCoder, SimpleCoder, Transaction},
    eips::eip4844::kzg_to_versioned_hash,
    providers::{Provider as _, ProviderBuilder, RootProvider},
    rpc::types::beacon::sidecar::BeaconBlobBundle,
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
    fn clone_box(&self) -> Box<dyn BlobReader>;
}

impl Clone for Box<dyn BlobReader> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone)]
pub struct ConsensusLayerBlobReader {
    provider: RootProvider,
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
            provider: ProviderBuilder::default().connect(ethereum_rpc).await?,
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
    fn clone_box(&self) -> Box<dyn BlobReader> {
        Box::new(self.clone())
    }

    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        //TODO: read genesis from `{ethereum_beacon_rpc}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1695902400;

        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or_else(|| anyhow!("failed to get transaction"))?;
        let blob_versioned_hashes = tx
            .blob_versioned_hashes()
            .ok_or_else(|| anyhow!("failed to get versioned hashes"))?;
        let blob_versioned_hashes = HashSet::<_, RandomState>::from_iter(blob_versioned_hashes);
        let block_hash = tx
            .block_hash
            .ok_or_else(|| anyhow!("failed to get block hash"))?;
        let block = self
            .provider
            .get_block_by_hash(block_hash)
            .await?
            .ok_or_else(|| anyhow!("failed to get block"))?;
        let slot =
            (block.header.timestamp - BEACON_GENESIS_BLOCK_TIME) / self.beacon_block_time.as_secs();
        let blob_bundle_result = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::trace!("trying to get blob, attempt #{}", count + 1);
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
}

impl MockBlobReader {
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_blob_transaction(&self, tx_hash: H256, data: Vec<u8>) {
        self.transactions.write().await.insert(tx_hash, data);
    }
}

impl Default for MockBlobReader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlobReader for MockBlobReader {
    fn clone_box(&self) -> Box<dyn BlobReader> {
        Box::new(self.clone())
    }

    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        let maybe_blob_data = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::trace!("trying to get blob, attempt #{}", count + 1);
                    let maybe_blob_data = self.transactions.read().await.get(&tx_hash).cloned();
                    if maybe_blob_data.is_some() || count >= attempts {
                        break maybe_blob_data;
                    } else {
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
