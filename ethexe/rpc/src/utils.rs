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

use crate::errors;
use anyhow::Result;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRO, BlockMetaStorageRO, LatestDataStorageRO, OnChainStorageRO},
};
use hyper::header::HeaderValue;
use jsonrpsee::core::RpcResult;
use sp_core::H256;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub(crate) fn try_into_cors(maybe_cors: Option<Vec<String>>) -> Result<CorsLayer> {
    if let Some(cors) = maybe_cors {
        let mut list = Vec::new();

        for origin in cors {
            list.push(HeaderValue::from_str(&origin)?)
        }

        Ok(CorsLayer::new().allow_origin(AllowOrigin::list(list)))
    } else {
        // allow all cors
        Ok(CorsLayer::permissive())
    }
}

pub fn block_at_or_latest<DB: BlockMetaStorageRO + OnChainStorageRO + LatestDataStorageRO>(
    db: &DB,
    at: impl Into<Option<H256>>,
) -> RpcResult<SimpleBlockData> {
    let hash = if let Some(hash) = at.into() {
        if !db.block_meta(hash).prepared {
            return Err(errors::db("Requested block is not prepared"));
        }
        hash
    } else {
        db.latest_data()
            .ok_or_else(|| errors::db("Latest data wasn't found"))?
            .prepared_block_hash
    };

    db.block_header(hash)
        .map(|header| SimpleBlockData { hash, header })
        .ok_or_else(|| errors::db("Block header for requested hash wasn't found"))
}

// TODO: #4948 not perfect solution, better to take the last synced block, and iterate back until
// found not expired announce from `at`, after commitment_delay_limit each block contains
// only one not expired announce. In current solution we can return expired announce in some cases.
/// Try to return latest computed announce hash or computed announce at given block hash.
/// If `at` contains many announces, then we prefer not-base one (if any), else take the first one.
pub fn announce_at_or_latest<DB: BlockMetaStorageRO + LatestDataStorageRO + AnnounceStorageRO>(
    db: &DB,
    at: impl Into<Option<H256>>,
) -> RpcResult<HashOf<Announce>> {
    if let Some(at) = at.into() {
        let computed_announces: Vec<_> = db
            .block_meta(at)
            .announces
            .into_iter()
            .flatten()
            .filter(|announce_hash| db.announce_meta(*announce_hash).computed)
            .collect();

        if let Some(non_base_announce) = computed_announces.iter().find(|&&announce_hash| {
            db.announce(announce_hash)
                .map(|a| !a.is_base())
                .unwrap_or_else(|| {
                    log::error!(
                        "Failed to get body for included announce {announce_hash}, at {at}"
                    );
                    false
                })
        }) {
            Ok(*non_base_announce)
        } else {
            computed_announces.into_iter().next().ok_or_else(|| {
                log::error!("No computed announces found at given block {at:?}");
                errors::db("No computed announces found at given block hash")
            })
        }
    } else {
        db.latest_data()
            .ok_or_else(|| errors::db("Latest data wasn't found"))
            .map(|data| data.computed_announce_hash)
    }
}
