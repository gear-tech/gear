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

// TODO #4552: add tests for observer utils

use alloy::{
    network::{Ethereum, Network},
    providers::{Provider as _, RootProvider},
    rpc::{
        client::BatchRequest,
        types::{
            Block, Log,
            eth::{Filter, Topic},
        },
    },
};
use anyhow::{Context, Result};
use ethexe_common::{Address, BlockData, BlockHeader, events::BlockEvent};
use ethexe_ethereum::{mirror, router, wvara};
use futures::future;
use gprimitives::H256;
use std::{collections::HashMap, future::IntoFuture, ops::RangeInclusive};

/// Max number of blocks to query in alloy.
const MAX_QUERY_BLOCK_RANGE: usize = 256;

#[allow(async_fn_in_trait)]
pub trait BlockLoader {
    async fn load(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData>;

    async fn load_many(&self, range: RangeInclusive<u64>) -> Result<HashMap<H256, BlockData>>;
}

#[derive(Debug, Clone)]
pub struct EthereumBlockLoader {
    provider: RootProvider,
    router_address: Address,
    wvara_address: Address,
}

impl EthereumBlockLoader {
    pub(crate) fn new(
        provider: RootProvider,
        router_address: Address,
        wvara_address: Address,
    ) -> Self {
        Self {
            provider,
            router_address,
            wvara_address,
        }
    }

    fn log_filter() -> Filter {
        let topic = Topic::from_iter(
            [
                router::events::signatures::ALL,
                wvara::events::signatures::ALL,
                mirror::events::signatures::ALL,
            ]
            .into_iter()
            .flatten()
            .copied(),
        );

        Filter::new().event_signature(topic)
    }

    fn logs_to_events(&self, logs: Vec<Log>) -> Result<HashMap<H256, Vec<BlockEvent>>> {
        let block_hash_of = |log: &Log| -> Result<H256> {
            log.block_hash
                .map(|v| v.0.into())
                .context("block hash is missing")
        };

        let mut res: HashMap<_, Vec<_>> = HashMap::new();

        for log in logs {
            let block_hash = block_hash_of(&log)?;
            let address = log.address();

            if address.0 == self.router_address.0 {
                if let Some(event) = router::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            } else if address.0 == self.wvara_address.0 {
                if let Some(event) = wvara::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            } else {
                let address = (*address.into_word()).into();

                if let Some(event) = mirror::events::try_extract_event(&log)? {
                    res.entry(block_hash)
                        .or_default()
                        .push(BlockEvent::mirror(address, event));
                }
            }
        }

        Ok(res)
    }

    fn block_response_to_data(block: Block) -> (H256, BlockHeader) {
        let block_hash = H256(block.header.hash.0);

        let header = BlockHeader {
            height: block.header.number as u32,
            timestamp: block.header.timestamp,
            parent_hash: H256(block.header.parent_hash.0),
        };

        (block_hash, header)
    }

    async fn request_block_batch(&self, range: RangeInclusive<u64>) -> Result<Vec<BlockData>> {
        let mut batch = BatchRequest::new(self.provider.client());
        let headers_request = range
            .clone()
            .map(|bn| {
                batch
                    .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                        "eth_getBlockByNumber",
                        &(format!("0x{bn:x}"), false),
                    )
                    .expect("infallible")
            })
            .collect::<Vec<_>>();
        batch.send().await?;
        let headers_request = future::join_all(headers_request);

        let filter = Self::log_filter()
            .from_block(*range.start())
            .to_block(*range.end());
        let logs_request = self.provider.get_logs(&filter);

        let (blocks, logs) = future::join(headers_request, logs_request).await;
        let logs = logs?;

        let mut blocks_data = Vec::new();
        for response in blocks {
            let block = response?;
            let Some(block) = block else {
                break;
            };

            let (block_hash, header) = Self::block_response_to_data(block);
            blocks_data.push(BlockData {
                hash: block_hash,
                header,
                events: Vec::new(),
            });
        }

        let mut events = self.logs_to_events(logs)?;
        for block_data in blocks_data.iter_mut() {
            block_data.events = events.remove(&block_data.hash).unwrap_or_default();
        }

        Ok(blocks_data)
    }
}

impl BlockLoader for EthereumBlockLoader {
    async fn load(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData> {
        log::trace!("Querying data for one block {block:?}");

        let filter = Self::log_filter().at_block_hash(block.0);
        let logs_request = self.provider.get_logs(&filter);

        let (block_hash, header, logs) = if let Some(header) = header {
            (block, header, logs_request.await?)
        } else {
            let block_request = self
                .provider
                .get_block_by_hash(block.0.into())
                .into_future();
            let (response, logs) = future::try_join(block_request, logs_request).await?;
            let response = response.context("block not found")?;
            let (block, header) = Self::block_response_to_data(response);
            (block, header, logs)
        };
        anyhow::ensure!(
            block_hash == block,
            "expected block hash {block}, got {block_hash}"
        );

        let events = self.logs_to_events(logs)?;
        anyhow::ensure!(
            events.len() <= 1,
            "expected events for at most 1 block, but got for {}",
            events.len()
        );

        let (block_hash, events) = events
            .into_iter()
            .next()
            .unwrap_or_else(|| (block_hash, Vec::new()));
        anyhow::ensure!(
            block_hash == block,
            "expected block hash {block}, got {block_hash}"
        );

        Ok(BlockData {
            hash: block,
            header,
            events,
        })
    }

    async fn load_many(&self, range: RangeInclusive<u64>) -> Result<HashMap<H256, BlockData>> {
        log::trace!("Querying blocks batch in {range:?} range");

        let batch_futures = range.clone().step_by(MAX_QUERY_BLOCK_RANGE).map(|start| {
            let end = (start + MAX_QUERY_BLOCK_RANGE as u64 - 1).min(*range.end());
            self.request_block_batch(start..=end)
        });

        let batches = future::try_join_all(batch_futures).await?;
        Ok(batches
            .into_iter()
            .flatten()
            .map(|data| (data.hash, data))
            .collect())
    }
}
