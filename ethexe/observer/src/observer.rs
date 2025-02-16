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
    rpc::types::{
        eth::{Filter, Topic},
        Block, Log,
    },
};
use anyhow::{anyhow, Result};
use ethexe_common::events::BlockEvent;
use ethexe_db::BlockHeader;
use ethexe_ethereum::{mirror, router, wvara};
use gear_core::ids::prelude::*;
use gprimitives::{CodeId, H256};
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

pub(crate) fn router_and_wvara_filter(
    filter: Filter,
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
) -> Filter {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::ALL
            .iter()
            .chain(wvara::events::signatures::ALL)
            .cloned(),
    );

    filter
        .clone()
        .address(vec![router_address, wvara_address])
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
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
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
            address if address == router_address => {
                if let Some(event) = router::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            }
            address if address == wvara_address => {
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
