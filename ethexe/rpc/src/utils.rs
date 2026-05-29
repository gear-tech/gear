// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

/// Latest MB whose per-row state is on disk.
pub fn latest_computed_mb(db: &Database) -> RpcResult<H256> {
    let hash = db.globals().latest_computed_mb_hash;
    if hash.is_zero() {
        return Err(errors::db(
            "no computed MB available yet; RPC reads require an MB-side state",
        ));
    }
    Ok(hash)
}
