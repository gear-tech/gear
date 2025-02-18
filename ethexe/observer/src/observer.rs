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

use crate::BlobReader;
use alloy::{
    primitives::Address as AlloyAddress,
    providers::{Provider as _, RootProvider},
    rpc::types::eth::{Filter, Topic},
};
use anyhow::{anyhow, Result};
use ethexe_common::events::{BlockEvent, BlockRequestEvent};
use ethexe_ethereum::{
    mirror,
    router::{self, RouterQuery},
    wvara,
};
use futures::future;
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
use std::{collections::HashMap, sync::Arc};

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

pub async fn read_block_events(
    block_hash: H256,
    provider: &RootProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockEvent>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let filter = Filter::new().at_block_hash(block_hash.to_fixed_bytes());

    read_events_impl(router_address, wvara_address, provider, filter)
        .await
        .map(|v| v.into_values().next().unwrap_or_default())
}

pub async fn read_block_events_batch(
    from_block: u32,
    to_block: u32,
    provider: &RootProvider,
    router_address: AlloyAddress,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let mut res = HashMap::new();

    let mut start_block = from_block as u64;
    let to_block = to_block as u64;

    while start_block <= to_block {
        let end_block = to_block.min(start_block + MAX_QUERY_BLOCK_RANGE as u64 - 1);

        let filter = Filter::new().from_block(start_block).to_block(end_block);

        let iter_res = read_events_impl(router_address, wvara_address, provider, filter).await?;

        res.extend(iter_res.into_iter());

        start_block = end_block + 1;
    }

    Ok(res)
}

async fn read_events_impl(
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
    provider: &RootProvider,
    filter: Filter,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::ALL
            .iter()
            .chain(wvara::events::signatures::ALL)
            .cloned(),
    );

    let router_and_wvara_filter = filter
        .clone()
        .address(vec![router_address, wvara_address])
        .event_signature(router_and_wvara_topic);

    let mirror_filter = filter.event_signature(Topic::from_iter(
        mirror::events::signatures::ALL.iter().cloned(),
    ));

    let (router_and_wvara_logs, mirrors_logs) = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirror_filter),
    )
    .await?;

    let block_hash_of = |log: &alloy::rpc::types::Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or_else(|| anyhow!("Block hash is missing"))
    };

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for router_or_wvara_log in router_and_wvara_logs {
        let block_hash = block_hash_of(&router_or_wvara_log)?;

        let maybe_block_event = if router_or_wvara_log.address() == router_address {
            router::events::try_extract_event(&router_or_wvara_log)?.map(Into::into)
        } else {
            wvara::events::try_extract_event(&router_or_wvara_log)?.map(Into::into)
        };

        if let Some(block_event) = maybe_block_event {
            res.entry(block_hash).or_default().push(block_event);
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        // TODO (breathx): if address is unknown, then continue.

        if let Some(event) = mirror::events::try_extract_event(&mirror_log)? {
            res.entry(block_hash)
                .or_default()
                .push(BlockEvent::mirror(address, event));
        }
    }

    Ok(res)
}

pub(crate) async fn read_block_request_events(
    block_hash: H256,
    provider: &RootProvider,
    router_address: AlloyAddress,
) -> Result<Vec<BlockRequestEvent>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let filter = Filter::new().at_block_hash(block_hash.to_fixed_bytes());

    read_request_events_impl(router_address, wvara_address, provider, filter)
        .await
        .map(|v| v.into_values().next().unwrap_or_default())
}

pub(crate) async fn read_block_request_events_batch(
    from_block: u32,
    to_block: u32,
    provider: &RootProvider,
    router_address: AlloyAddress,
) -> Result<HashMap<H256, Vec<BlockRequestEvent>>> {
    let router_query = RouterQuery::from_provider(router_address, Arc::new(provider.clone()));
    let wvara_address = router_query.wvara_address().await?;

    let mut res = HashMap::new();

    let mut start_block = from_block as u64;
    let to_block = to_block as u64;

    // TODO (breathx): FIX WITHIN PR. to iters.
    while start_block <= to_block {
        let end_block = to_block.min(start_block + MAX_QUERY_BLOCK_RANGE as u64 - 1);

        let filter = Filter::new().from_block(start_block).to_block(end_block);

        let iter_res =
            read_request_events_impl(router_address, wvara_address, provider, filter).await?;

        res.extend(iter_res.into_iter());

        start_block = end_block + 1;
    }

    Ok(res)
}

async fn read_request_events_impl(
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
    provider: &RootProvider,
    filter: Filter,
) -> Result<HashMap<H256, Vec<BlockRequestEvent>>> {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::REQUESTS
            .iter()
            .chain(wvara::events::signatures::REQUESTS)
            .cloned(),
    );

    let router_and_wvara_filter = filter
        .clone()
        .address(vec![router_address, wvara_address])
        .event_signature(router_and_wvara_topic);

    let mirror_filter = filter.event_signature(Topic::from_iter(
        mirror::events::signatures::REQUESTS.iter().cloned(),
    ));

    let (router_and_wvara_logs, mirrors_logs) = future::try_join(
        provider.get_logs(&router_and_wvara_filter),
        provider.get_logs(&mirror_filter),
    )
    .await?;

    let block_hash_of = |log: &alloy::rpc::types::Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let out_of_scope_addresses = [
        (*router_address.into_word()).into(),
        (*wvara_address.into_word()).into(),
        ActorId::zero(),
    ];

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for router_or_wvara_log in router_and_wvara_logs {
        let block_hash = block_hash_of(&router_or_wvara_log)?;

        let maybe_block_request_event = if router_or_wvara_log.address() == router_address {
            router::events::try_extract_request_event(&router_or_wvara_log)?.map(Into::into)
        } else {
            wvara::events::try_extract_request_event(&router_or_wvara_log)?
                .filter(|v| !v.involves_addresses(&out_of_scope_addresses))
                .map(Into::into)
        };

        if let Some(block_request_event) = maybe_block_request_event {
            res.entry(block_hash).or_default().push(block_request_event);
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        // TODO (breathx): if address is unknown, then continue.

        if let Some(request_event) = mirror::events::try_extract_request_event(&mirror_log)? {
            res.entry(block_hash)
                .or_default()
                .push(BlockRequestEvent::mirror(address, request_event));
        }
    }

    Ok(res)
}

pub(crate) async fn read_committed_blocks_batch(
    from_block: u32,
    to_block: u32,
    provider: &RootProvider,
    router_address: AlloyAddress,
) -> Result<Vec<H256>> {
    let mut start_block = from_block as u64;
    let to_block = to_block as u64;

    let mut res = Vec::new();

    while start_block <= to_block {
        let end_block = to_block.min(start_block + MAX_QUERY_BLOCK_RANGE as u64 - 1);

        let filter = Filter::new().from_block(start_block).to_block(end_block);

        let iter_res = read_committed_blocks_impl(router_address, provider, filter).await?;

        res.extend(iter_res);

        start_block = end_block + 1;
    }

    Ok(res)
}

async fn read_committed_blocks_impl(
    router_address: AlloyAddress,
    provider: &RootProvider,
    filter: Filter,
) -> Result<Vec<H256>> {
    let filter = filter
        .address(router_address)
        .event_signature(Topic::from(router::events::signatures::BLOCK_COMMITTED));

    let logs = provider.get_logs(&filter).await?;

    let mut res = Vec::with_capacity(logs.len());

    for log in logs {
        if let Some(hash) = router::events::try_extract_committed_block_hash(&log)? {
            res.push(hash);
        }
    }

    Ok(res)
}
