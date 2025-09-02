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
    consensus::BlockHeader,
    eips::BlockNumberOrTag,
    network::{BlockResponse, Ethereum, Network},
    providers::{Provider, RootProvider},
    rpc::{
        client::BatchRequest,
        types::{
            Block, Log,
            eth::{Filter, Topic},
        },
    },
    transports::{BoxFuture, RpcError, TransportErrorKind},
};
use anyhow::{Result, anyhow};
use ethexe_common::{Address, BlockData, BlockHeader, events::BlockEvent};
use ethexe_ethereum::{mirror, router, wvara};
use futures::{FutureExt, Stream, future, stream::FuturesUnordered};
use gprimitives::H256;
use std::{collections::HashMap, future::IntoFuture};

type GetBlockFuture<N: Network> =
    BoxFuture<'static, Result<Option<N::BlockResponse>, RpcError<TransportErrorKind>>>;
type SleepFuture = BoxFuture<'static, ()>;

pub(crate) struct FinalizedBlocksStream<P, N: Network = Ethereum> {
    // Control flow futures
    fut: Option<GetBlockFuture<N>>,
    sleep_fut: Option<BoxFuture<'statis, ()>>,

    // Cached d
    previous_finalized: Option<N::BlockResponse>,

    provider: P,
}

impl<P: Provider<N> + Clone, N: Network> FinalizedBlocksStream<P, N> {
    pub async fn new(provider: P) -> Result<Self> {
        let previous_finalized = provider
            .get_block_by_number(BlockNumberOrTag::Finalized)
            .await?
            .unwrap();
        let n = previous_finalized.header();

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).into_future();
    }
}

impl<P, N> Stream for FinalizedBlocksStream<P>
where
    P: Provider<N> + Clone,
    N: Network,
{
    type Item = Result<N::Header>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Finalized)
            .into_future();
    }
}

/// Max number of blocks to query in alloy.
pub(crate) const MAX_QUERY_BLOCK_RANGE: usize = 256;

pub(crate) fn log_filter() -> Filter {
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

pub(crate) fn logs_to_events(
    logs: Vec<Log>,
    router_address: Address,
    wvara_address: Address,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let block_hash_of = |log: &Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for log in logs {
        let block_hash = block_hash_of(&log)?;
        let address = log.address();

        if address.0 == router_address.0 {
            if let Some(event) = router::events::try_extract_event(&log)? {
                res.entry(block_hash).or_default().push(event.into());
            }
        } else if address.0 == wvara_address.0 {
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

pub(crate) fn block_response_to_data(response: Option<Block>) -> Result<(H256, BlockHeader)> {
    let block = response.ok_or_else(|| anyhow!("Block not found"))?;
    let block_hash = H256(block.header.hash.0);

    let header = BlockHeader {
        height: block.header.number as u32,
        timestamp: block.header.timestamp,
        parent_hash: H256(block.header.parent_hash.0),
    };

    Ok((block_hash, header))
}

pub(crate) async fn load_block_data(
    provider: RootProvider,
    block: H256,
    router_address: Address,
    wvara_address: Address,
    header: Option<BlockHeader>,
) -> Result<BlockData> {
    log::trace!("Querying data for one block {block:?}");

    let filter = log_filter().at_block_hash(block.0);
    let logs_request = provider.get_logs(&filter);

    let ((block_hash, header), logs) = if let Some(header) = header {
        ((block, header), logs_request.await?)
    } else {
        let block_request = provider.get_block_by_hash(block.0.into()).into_future();

        match future::try_join(block_request, logs_request).await {
            Ok((response, logs)) => (block_response_to_data(response)?, logs),
            Err(err) => Err(err)?,
        }
    };

    if block_hash != block {
        return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
    }

    let events = logs_to_events(logs, router_address, wvara_address)?;

    if events.len() > 1 {
        return Err(anyhow!(
            "Expected events for at most 1 block, but got for {}",
            events.len()
        ));
    }

    let (block_hash, events) = events
        .into_iter()
        .next()
        .unwrap_or_else(|| (block_hash, Vec::new()));

    if block_hash != block {
        return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
    }

    Ok(BlockData {
        hash: block,
        header,
        events,
    })
}

pub(crate) async fn load_blocks_data_batched(
    provider: RootProvider,
    from_block: u64,
    to_block: u64,
    router_address: Address,
    wvara_address: Address,
) -> Result<HashMap<H256, BlockData>> {
    let batch_futures: FuturesUnordered<_> = (from_block..=to_block)
        .step_by(MAX_QUERY_BLOCK_RANGE)
        .map(|start| {
            let end = (start + MAX_QUERY_BLOCK_RANGE as u64 - 1).min(to_block);

            load_blocks_batch_data(provider.clone(), router_address, wvara_address, start, end)
                .boxed()
        })
        .collect();

    future::try_join_all(batch_futures).await.map(|batches| {
        batches
            .into_iter()
            .flat_map(|batch| {
                batch
                    .into_iter()
                    .map(|block_data| (block_data.hash, block_data))
            })
            .collect()
    })
}

async fn load_blocks_batch_data(
    provider: RootProvider,
    router_address: Address,
    wvara_address: Address,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<BlockData>> {
    log::trace!("Querying blocks batch from {from_block} to {to_block}");

    let mut batch = BatchRequest::new(provider.client());

    let headers_request: FuturesUnordered<_> = (from_block..=to_block)
        .map(|bn| {
            batch
                .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                    "eth_getBlockByNumber",
                    &(format!("0x{bn:x}"), false),
                )
                .expect("infallible")
                .boxed()
        })
        .collect();

    batch.send().await?;

    let filter = log_filter().from_block(from_block).to_block(to_block);
    let logs_request = provider.get_logs(&filter);

    let (blocks, logs) = future::join(future::join_all(headers_request), logs_request).await;
    let logs = logs?;

    let mut blocks_data = Vec::new();

    for response in blocks {
        let block = response?.ok_or_else(|| anyhow!("Block not found"))?;
        let block_hash = H256(block.header.hash.0);

        let header = BlockHeader {
            height: block.header.number as u32,
            timestamp: block.header.timestamp,
            parent_hash: H256(block.header.parent_hash.0),
        };

        blocks_data.push(BlockData {
            hash: block_hash,
            header,
            events: Vec::new(),
        });
    }

    let mut events = logs_to_events(logs, router_address, wvara_address)?;
    for block_data in blocks_data.iter_mut() {
        block_data.events = events.remove(&block_data.hash).unwrap_or_default();
    }

    Ok(blocks_data)
}
