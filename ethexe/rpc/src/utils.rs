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
use ethexe_common::{
    SimpleBlockData,
    db::{GlobalsStorageRO, OnChainStorageRO},
};
use ethexe_db::Database;
use jsonrpsee::core::RpcResult;
use sp_core::H256;

pub fn block_at_or_latest_synced(
    db: &Database,
    at: impl Into<Option<H256>>,
) -> RpcResult<SimpleBlockData> {
    let hash = if let Some(hash) = at.into() {
        if !db.block_synced(hash) {
            return Err(errors::db("Requested block is not synced"));
        }
        hash
    } else {
        db.globals().latest_synced_eb.hash
    };

    db.block_header(hash)
        .map(|header| SimpleBlockData { hash, header })
        .ok_or_else(|| errors::db("Block header for requested hash wasn't found"))
}

/// Returns the most recently finalized Malachite-block hash for serving
/// MB-based RPC reads (program states, outcome, schedule).
///
/// `H256::zero()` is returned as an error — callers cannot serve
/// a meaningful answer before any MB has been finalized.
pub fn latest_finalized_mb(db: &Database) -> RpcResult<H256> {
    let hash = db.globals().latest_finalized_mb_hash;
    if hash.is_zero() {
        return Err(errors::db(
            "no finalized MB available yet; RPC reads require an MB-side state",
        ));
    }
    Ok(hash)
}
