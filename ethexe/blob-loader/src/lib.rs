// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::blobs::{BlobData, BlobReader};
use anyhow::{anyhow, Result};
use ethexe_common::db::{CodesStorage, OnChainStorage};
use ethexe_db::Database;
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::CodeId;
use std::{collections::HashSet, fmt, pin::Pin, task::Poll};

use utils::*;

pub mod local;
pub mod utils;

#[derive(Clone, PartialEq, Eq)]
pub struct BlobData {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub code: Vec<u8>,
}

impl fmt::Debug for BlobData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BlobData")
            .field("code_id", &self.code_id)
            .field("timestamp", &self.timestamp)
            .field("code", &format_args!("{} bytes", self.code.len()))
            .finish()
    }
}

#[derive(Clone)]
pub enum BlobLoaderEvent {
    BlobLoaded(BlobData),
}

pub trait BlobLoaderService {
    fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()>;
}

impl fmt::Debug for BlobLoaderEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlobLoaderEvent::BlobLoaded(data) => data.fmt(f),
        }
    }
}

pub struct BlobLoader {
    futures: FuturesUnordered<BoxFuture<'static, Result<BlobData>>>,
    codes_loading: HashSet<CodeId>,

    blob_reader: Box<dyn BlobReader>,
    db: Database,
}

impl Stream for BlobLoader {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let future = self.futures.poll_next_unpin(cx);
        match future {
            Poll::Ready(Some(result)) => match result {
                Ok(blob_data) => {
                    let code_id = &blob_data.code_id;
                    self.codes_loading.remove(code_id);
                    self.db.set_original_code(blob_data.code.as_slice());
                    Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(blob_data))))
                }
                Err(e) => Poll::Ready(Some(Err(e))),
            },
            _ => Poll::Pending,
        }
    }
}

impl FusedStream for BlobLoader {
    fn is_terminated(&self) -> bool {
        false
    }
}

// #[derive(Clone)]
// pub struct ConsensusLayerBlobReader {
//     provider: RootProvider,
//     http_client: Client,
//     ethereum_beacon_rpc: String,
//     beacon_block_time: Duration,
// }

impl BlobLoader {
    pub fn new(db: Database) -> Self {
        Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),
            blob_reader,
            db,
        }
    }

    async fn read_code_from_tx_hash(&mut self) -> Result<BlobData> {}

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

impl BlobLoaderService for BlobLoader {
    fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()> {
        log::info!("Request load codes: {codes:?}");
        for code_id in codes {
            if self.codes_loading.contains(&code_id) || self.db.original_code_exists(code_id) {
                continue;
            }

            let code_info = self
                .db
                .code_blob_info(code_id)
                .ok_or_else(|| anyhow!("Not found {code_id} in db"))?;

            self.codes_loading.insert(code_id);
            self.futures.push(
                crate::read_code_from_tx_hash(
                    self.blob_reader.clone(),
                    code_id,
                    code_info.timestamp,
                    code_info.tx_hash,
                    attempts,
                )
                .boxed(),
            );
        }

        Ok(())
    }
}
