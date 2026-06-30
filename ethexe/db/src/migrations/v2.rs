// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Migration v1 -> v2.
//!
//! `MbMeta` gained a `finalized` flag, and its `last_advanced_eb` changed from
//! `H256` to `Option<H256>`. Every stored `MbMeta` record is re-encoded: the
//! old `last_advanced_eb` is wrapped into `Some(..)`, and `finalized` is
//! backfilled to `true` for every MB reachable from `latest_finalized_mb_hash`
//! — mirroring the old reachability-based "finalized locally" semantics.

use super::{InitConfig, migration::Migration, v1};
use crate::RawDatabase;
use anyhow::{Context, Result};
use ethexe_common::db::{CompactMb, DBConfig, DBGlobals};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{collections::HashSet, pin::Pin};

pub const VERSION: u32 = 2;

// Discriminants frozen from `database::Key` at the time of this migration.
// They must not be changed even if the live `Key` enum is renumbered later.
const MB_META: u64 = 22;
const MB_COMPACT_BLOCK: u64 = 25;
const GLOBALS: u64 = 14;
const CONFIG: u64 = 15;

fn prefix(discriminant: u64) -> [u8; 32] {
    H256::from_low_u64_be(discriminant).to_fixed_bytes()
}

fn hash_key(discriminant: u64, hash: H256) -> Vec<u8> {
    let mut key = prefix(discriminant).to_vec();
    key.extend_from_slice(hash.as_ref());
    key
}

fn singleton_key(discriminant: u64) -> Vec<u8> {
    let mut key = prefix(discriminant).to_vec();
    key.extend_from_slice(&[0u8; 8]);
    key
}

/// Frozen v2 layout of `MbMeta`.
#[derive(Encode)]
struct MbMetaV2 {
    computed: bool,
    finalized: bool,
    last_advanced_eb: Option<H256>,
}

pub struct MigrationFromV1;

impl Migration for MigrationFromV1 {
    fn source_version(&self) -> u32 {
        VERSION - 1
    }

    fn migrate<'a>(
        &'a self,
        _config: &'a InitConfig,
        db: &'a RawDatabase,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            let finalized = collect_finalized_mbs(db)?;

            let meta_prefix = prefix(MB_META);
            let entries: Vec<(Vec<u8>, Vec<u8>)> = db.kv.iter_prefix(&meta_prefix).collect();
            for (key, value) in entries {
                let mb_hash = H256::from_slice(&key[meta_prefix.len()..]);
                let old = v1::MbMeta::decode(&mut value.as_slice())
                    .with_context(|| format!("failed to decode v1 MbMeta for {mb_hash}"))?;
                let new = MbMetaV2 {
                    computed: old.computed,
                    finalized: finalized.contains(&mb_hash),
                    last_advanced_eb: Some(old.last_advanced_eb),
                };
                db.kv.put(&key, new.encode());
            }

            // Bump the persisted schema version to v2.
            let mut config = DBConfig::decode(
                &mut db
                    .kv
                    .get(&singleton_key(CONFIG))
                    .context("config not found during v1 -> v2 migration")?
                    .as_slice(),
            )
            .context("failed to decode config during v1 -> v2 migration")?;
            config.version = VERSION;
            db.kv.put(&singleton_key(CONFIG), config.encode());

            Ok(())
        })
    }
}

/// Collect every MB hash reachable from `latest_finalized_mb_hash` by walking
/// `CompactMb::parent`, plus the genesis zero MB. The walk terminates at the
/// zero sentinel, a missing compact block, or a cycle.
fn collect_finalized_mbs(db: &RawDatabase) -> Result<HashSet<H256>> {
    let globals = DBGlobals::decode(
        &mut db
            .kv
            .get(&singleton_key(GLOBALS))
            .context("globals not found during v1 -> v2 migration")?
            .as_slice(),
    )
    .context("failed to decode globals during v1 -> v2 migration")?;

    let mut finalized = HashSet::new();
    let mut current = globals.latest_finalized_mb_hash;
    while !current.is_zero() && finalized.insert(current) {
        let Some(raw) = db.kv.get(&hash_key(MB_COMPACT_BLOCK, current)) else {
            break;
        };
        let compact = CompactMb::decode(&mut raw.as_slice())
            .with_context(|| format!("failed to decode CompactMb for {current}"))?;
        current = compact.parent;
    }
    // The genesis zero MB has its own `MbMeta` row and is finalized by definition.
    finalized.insert(H256::zero());
    Ok(finalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemDb, RawDatabase};
    use ethexe_common::{Address, ProtocolTimelines, SimpleBlockData};

    fn put_v1_meta(db: &RawDatabase, hash: H256, computed: bool, last_advanced_eb: H256) {
        // SCALE-encodes identically to the frozen v1 `MbMeta` struct.
        db.kv.put(
            &hash_key(MB_META, hash),
            (computed, last_advanced_eb).encode(),
        );
    }

    fn put_compact(db: &RawDatabase, hash: H256, parent: H256) {
        let compact = CompactMb {
            parent,
            height: 0,
            operations_hash: H256::zero(),
        };
        db.kv
            .put(&hash_key(MB_COMPACT_BLOCK, hash), compact.encode());
    }

    #[test]
    fn migrate_reencodes_mb_meta_and_backfills_finalized() {
        let db = RawDatabase::from_one(&MemDb::default());

        // Chain: zero <- mb1 <- mb2 (finalized head) ; mb3 is a computed,
        // not-yet-finalized tip above mb2.
        let mb1 = H256::from_low_u64_be(0x1);
        let mb2 = H256::from_low_u64_be(0x2);
        let mb3 = H256::from_low_u64_be(0x3);
        put_compact(&db, mb1, H256::zero());
        put_compact(&db, mb2, mb1);
        put_compact(&db, mb3, mb2);

        put_v1_meta(&db, H256::zero(), true, H256::zero());
        put_v1_meta(&db, mb1, true, H256::from_low_u64_be(0xE1));
        put_v1_meta(&db, mb2, true, H256::from_low_u64_be(0xE2));
        put_v1_meta(&db, mb3, true, H256::from_low_u64_be(0xE3));

        let globals = DBGlobals {
            start_block_hash: H256::zero(),
            latest_synced_eb: SimpleBlockData::default(),
            latest_prepared_eb_hash: H256::zero(),
            latest_finalized_mb_hash: mb2,
            latest_computed_mb_hash: mb3,
        };
        db.kv.put(&singleton_key(GLOBALS), globals.encode());

        let config = DBConfig {
            version: 1,
            chain_id: 0,
            router_address: Address([0; 20]),
            timelines: ProtocolTimelines {
                genesis_ts: 0,
                era: 1.try_into().unwrap(),
                election: 0,
                slot: 1.try_into().unwrap(),
            },
            genesis_block_hash: H256::zero(),
            max_validators: 10,
        };
        db.kv.put(&singleton_key(CONFIG), config.encode());

        let init_config = InitConfig {
            ethereum_rpc: String::new(),
            router_address: Address([0; 20]),
            slot_duration_secs: 1,
            genesis_initializer: None,
        };
        futures::executor::block_on(MigrationFromV1.migrate(&init_config, &db)).unwrap();

        let decode_meta = |hash: H256| {
            let raw = db.kv.get(&hash_key(MB_META, hash)).unwrap();
            ethexe_common::db::MbMeta::decode(&mut raw.as_slice()).unwrap()
        };

        // Finalized chain (zero, mb1, mb2) is backfilled true; the tip mb3 false.
        assert!(decode_meta(H256::zero()).finalized);
        assert!(decode_meta(mb1).finalized);
        assert!(decode_meta(mb2).finalized);
        assert!(!decode_meta(mb3).finalized);

        // `last_advanced_eb` is wrapped into `Some(..)`.
        assert_eq!(
            decode_meta(mb1).last_advanced_eb,
            Some(H256::from_low_u64_be(0xE1))
        );

        // Version bumped to v2.
        let migrated_config =
            DBConfig::decode(&mut db.kv.get(&singleton_key(CONFIG)).unwrap().as_slice()).unwrap();
        assert_eq!(migrated_config.version, VERSION);
    }
}
