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
    consensus::BlockHeader as _,
    eips::BlockNumberOrTag,
    network::{BlockResponse, Ethereum, Network, primitives::HeaderResponse},
    providers::{Provider, RootProvider},
    rpc::{
        client::BatchRequest,
        types::{
            Block, Log,
            eth::{Filter, Topic},
        },
    },
    transports::{RpcError, TransportErrorKind},
};
use anyhow::{Result, anyhow};
use ethexe_common::{Address, BlockData, BlockHeader, events::BlockEvent};
use ethexe_ethereum::{mirror, router, wvara};
use futures::{FutureExt, Stream, future, future::BoxFuture, stream::FuturesUnordered};
use gprimitives::H256;
use std::{
    collections::HashMap,
    future::IntoFuture,
    ops::Mul,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

type GetBlockFuture<N> =
    BoxFuture<'static, Result<Option<<N as Network>::BlockResponse>, RpcError<TransportErrorKind>>>;

/// [`FinalizedBlocksStream`] returns finalized blocks as they become available.
/// It designed to minimize the number of requests to the provider.
/// It does so by:
/// - Waiting for the approximate time when the next finalized block is expected to be available.
/// - If the block is not yet available, it waits for the next slot duration before trying
/// NOTE: This is not a standart stream provided by the RPC node.
pub(crate) struct FinalizedBlocksStream<P, N: Network = Ethereum> {
    // Control flow futures
    get_block_fut: Option<GetBlockFuture<N>>,
    sleep_fut: Option<BoxFuture<'static, ()>>,
    // The latest finalized block we have seen
    latest_finalized: N::BlockResponse,

    provider: P,
}

impl<P: Provider<N> + Clone, N: Network> FinalizedBlocksStream<P, N> {
    pub async fn new(provider: P) -> Result<Self> {
        let latest_finalized = provider
            .get_block_by_number(BlockNumberOrTag::Finalized)
            .await?
            .unwrap();

        let mut this = Self {
            get_block_fut: None,
            sleep_fut: None,
            latest_finalized,
            provider,
        };

        // Turn to the waiting state
        this.wait_for_next_finalized_block();
        Ok(this)
    }

    // Set up a future to wait until the next finalized block is expected to be available.
    fn wait_for_next_finalized_block(&mut self) {
        let current_ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => unreachable!("Block timestamp can not be earlier than UNIX_EPOCH"),
        };

        let time_spent = current_ts.saturating_sub(self.latest_finalized.header().timestamp());

        // We assume that the blocks become finalized approximately every 2 epochs.
        self.wait_for(Duration::from_secs(
            alloy::eips::merge::EPOCH_DURATION_SECS
                .mul(2)
                .saturating_sub(time_spent),
        ));
    }

    // Wait for the next slot duration before trying again.
    fn wait_for_next_slot(&mut self) {
        self.wait_for(Duration::from_secs(alloy::eips::merge::SLOT_DURATION_SECS));
    }

    // Set up a future to wait.
    fn wait_for(&mut self, duration: Duration) {
        self.sleep_fut = Some(tokio::time::sleep(duration).into_future().boxed());
    }
}

impl<P, N> Stream for FinalizedBlocksStream<P, N>
where
    P: Provider<N> + Clone + std::marker::Unpin,
    N: Network,
{
    type Item = N::BlockResponse;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();

        if let Some(fut) = this.sleep_fut.as_mut() {
            let _: () = futures::ready!(fut.poll_unpin(cx));
            this.sleep_fut = None;

            let fut = this
                .provider
                .clone()
                .get_block_by_number(BlockNumberOrTag::Finalized)
                .into_future()
                .boxed();
            this.get_block_fut = Some(fut);
        }

        let Some(fut) = this.get_block_fut.as_mut() else {
            return Poll::Pending;
        };

        let maybe_block = match futures::ready!(fut.poll_unpin(cx)) {
            Ok(maybe_block) => maybe_block,
            Err(_) => {
                unimplemented!();
            }
        };
        this.get_block_fut = None;

        let block = match maybe_block {
            Some(block) if block.header().hash() != this.latest_finalized.header().hash() => block,
            _ => {
                // Wait for the next slot and try again.
                this.wait_for_next_slot();
                return Poll::Pending;
            }
        };

        this.latest_finalized = block.clone();
        this.wait_for_next_finalized_block();
        return Poll::Ready(Some(block));
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
