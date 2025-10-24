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
    AnnounceHash, SimpleBlockData,
    db::{BlockMetaStorageRead, LatestDataStorageRead, OnChainStorageRead},
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

pub fn block_header_at_or_latest<
    DB: BlockMetaStorageRead + OnChainStorageRead + LatestDataStorageRead,
>(
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

/// NOTE: does not return latest computed announce - instead use announce from latest prepared block.
pub fn announce_at_or_latest<
    DB: BlockMetaStorageRead + OnChainStorageRead + LatestDataStorageRead,
>(
    db: &DB,
    at: impl Into<Option<H256>>,
) -> RpcResult<AnnounceHash> {
    let block_hash = block_header_at_or_latest(db, at)?.hash;

    db.block_meta(block_hash)
        .announces
        .ok_or_else(|| {
            log::error!("Prepared block meta doesn't contain announces");
            errors::db("Block meta doesn't contain announces, can't get announce outcome")
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            log::error!("Prepared block meta doesn't contain any announces");
            errors::db("Block meta doesn't contain any announces, can't get announce outcome")
        })
}
