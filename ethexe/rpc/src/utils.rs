// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::errors;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRO, GlobalsStorageRO, OnChainStorageRO},
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
        db.globals().latest_synced_block.hash
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
pub fn announce_at_or_latest_computed(
    db: &Database,
    at: impl Into<Option<H256>>,
) -> RpcResult<HashOf<Announce>> {
    if let Some(at) = at.into() {
        let computed_announces: Vec<_> = db
            .block_announces(at)
            .into_iter()
            .flatten()
            .filter(|announce_hash| db.announce_meta(*announce_hash).computed)
            .collect();

        if let Some(non_base_announce) = computed_announces.iter().find(|&&announce_hash| {
            db.announce(announce_hash)
                .map(|a| !a.is_base())
                .unwrap_or_else(|| {
                    tracing::error!(
                        "Failed to get body for included announce {announce_hash}, at {at}"
                    );
                    false
                })
        }) {
            Ok(*non_base_announce)
        } else {
            computed_announces.into_iter().next().ok_or_else(|| {
                tracing::error!("No computed announces found at given block {at:?}");
                errors::db("No computed announces found at given block hash")
            })
        }
    } else {
        Ok(db.globals().latest_computed_announce_hash)
    }
}
