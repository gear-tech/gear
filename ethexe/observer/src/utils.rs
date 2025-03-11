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

// TODO (gsobol): add tests for utils

use crate::BlobReader;
use alloy::{
    network::{Ethereum, Network},
    providers::{Provider as _, RootProvider},
    rpc::{
        client::BatchRequest,
        types::{
            eth::{Filter, Topic},
            Block, Log,
        },
    },
};
use anyhow::{anyhow, Result};
use ethexe_common::{events::BlockEvent, BlockData};
use ethexe_db::BlockHeader;
use ethexe_ethereum::{mirror, router, wvara};
use ethexe_signer::Address;
use futures::{future, stream::FuturesUnordered, FutureExt};
use gear_core::ids::prelude::*;
use gprimitives::{CodeId, H256};
use std::{collections::HashMap, future::IntoFuture, sync::Arc};

/// Max number of blocks to query in alloy.
pub(crate) const MAX_QUERY_BLOCK_RANGE: usize = 256;

pub(crate) async fn read_code_from_tx_hash(
    blob_reader: Arc<dyn BlobReader>,
    expected_code_id: CodeId,
    timestamp: u64,
    tx_hash: H256,
    attempts: Option<u8>,
) -> Result<(CodeId, u64, Vec<u8>)> {
    let code = blob_reader
        .read_blob_from_tx_hash(tx_hash, attempts)
        .await
        .map_err(|err| anyhow!("failed to read blob: {err}"))?;

    (CodeId::generate(&code) == expected_code_id)
        .then_some(())
        .ok_or_else(|| anyhow!("unexpected code id"))?;

    Ok((expected_code_id, timestamp, code))
}

pub(crate) fn router_and_wvara_filter(
    filter: Filter,
    router_address: Address,
    wvara_address: Address,
) -> Filter {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::ALL
            .iter()
            .chain(wvara::events::signatures::ALL)
            .cloned(),
    );

    filter
        .clone()
        .address(vec![router_address.0.into(), wvara_address.0.into()])
        .event_signature(router_and_wvara_topic)
}

pub(crate) fn mirrors_filter(filter: Filter) -> Filter {
    filter.event_signature(Topic::from_iter(
        mirror::events::signatures::ALL.iter().cloned(),
    ))
}

pub(crate) fn logs_to_events(
    router_and_wvara_logs: Vec<Log>,
    mirrors_logs: Vec<Log>,
    router_address: Address,
    wvara_address: Address,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let block_hash_of = |log: &Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for log in router_and_wvara_logs {
        let block_hash = block_hash_of(&log)?;

        match log.address() {
            address if address.0 == router_address.0 => {
                if let Some(event) = router::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            }
            address if address.0 == wvara_address.0 => {
                if let Some(event) = wvara::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            }
            _ => unreachable!("Unexpected address in log"),
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        if let Some(event) = mirror::events::try_extract_event(&mirror_log)? {
            res.entry(block_hash)
                .or_default()
                .push(BlockEvent::mirror(address, event));
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

    let filter = Filter::new().at_block_hash(block.0);
    let mirrors_filter = crate::mirrors_filter(filter.clone());
    let router_and_wvara_filter =
        crate::router_and_wvara_filter(filter, router_address, wvara_address);

    let logs_request = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirrors_filter),
    );

    let ((block_hash, header), (router_and_wvara_logs, mirrors_logs)) = if let Some(header) = header
    {
        ((block, header), logs_request.await?)
    } else {
        let block_request = provider.get_block_by_hash(block.0.into()).into_future();

        match future::try_join(block_request, logs_request).await {
            Ok((response, logs)) => (crate::block_response_to_data(response)?, logs),
            Err(err) => Err(err)?,
        }
    };

    if block_hash != block {
        return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
    }

    let events = crate::logs_to_events(
        router_and_wvara_logs,
        mirrors_logs,
        router_address,
        wvara_address,
    )?;

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

    let filter = Filter::new().from_block(from_block).to_block(to_block);

    let mirrors_filter = crate::mirrors_filter(filter.clone());
    let router_and_wvara_filter =
        crate::router_and_wvara_filter(filter, router_address, wvara_address);

    let logs_request = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirrors_filter),
    );

    let (blocks, logs) = future::join(future::join_all(headers_request), logs_request).await;
    let (router_and_wvara_logs, mirrors_logs) = logs?;

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

    let mut events = crate::logs_to_events(
        router_and_wvara_logs,
        mirrors_logs,
        router_address,
        wvara_address,
    )?;
    for block_data in blocks_data.iter_mut() {
        block_data.events = events.remove(&block_data.hash).unwrap_or_default();
    }

    Ok(blocks_data)
}
