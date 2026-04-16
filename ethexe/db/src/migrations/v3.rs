use crate::{InitConfig, RawDatabase, database::BlockSmallData, migrations::v0};
use anyhow::{Context, Result};
use ethexe_common::db::{AnnounceStorageRW, BlockMeta, DBConfig};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

pub const VERSION: u32 = 3;

// Critical usages for migration
#[allow(unused_imports)]
use crate::KVDatabase;

const _: () = const {
    assert!(
        crate::VERSION == VERSION,
        "Check migration code for types changing in case of version change: DBConfig, BlockSmallData, v0::BlockSmallData, v0::BlockMeta. \
         Also check AnnounceStorageRW, KVDatabase, dyn KVDatabase implementations"
    );
};

pub async fn migration_from_v2(_: &InitConfig, db: &RawDatabase) -> Result<()> {
    // Changes from v2 to v3:
    // - Block announces are moved from `BlockMeta` to `BlockAnnounces` key.

    log::info!("Migration v2->v3: moving block announces from BlockMeta to BlockAnnounces key");

    const BLOCK_SMALL_DATA_PREFIX: u64 = 0x00;
    let mut blocks_migrated = 0u64;

    for (k, v) in db
        .kv
        .iter_prefix(H256::from_low_u64_be(BLOCK_SMALL_DATA_PREFIX).as_bytes())
    {
        if k.len() != 2 * std::mem::size_of::<H256>() {
            continue;
        }

        let block_hash = H256::from_slice(&k[std::mem::size_of::<H256>()..]);

        let old_data = v0::BlockSmallData::decode(&mut v.as_slice())
            .context("failed to decode BlockSmallData during migration")?;

        // Move announces to separate BlockAnnounces key
        if let Some(announces) = &old_data.meta.announces {
            db.set_block_announces(block_hash, announces.clone());
        }

        // Re-encode with new format (BlockMeta without announces)
        let new_data = BlockSmallData {
            block_header: old_data.block_header,
            block_is_synced: old_data.block_is_synced,
            meta: BlockMeta {
                prepared: old_data.meta.prepared,
                codes_queue: old_data.meta.codes_queue,
                last_committed_batch: old_data.meta.last_committed_batch,
                last_committed_announce: old_data.meta.last_committed_announce,
            },
        };

        db.kv.put(&k, new_data.encode());
        blocks_migrated += 1;
    }

    log::info!("Migration v2->v3: migrated {blocks_migrated} blocks");

    let config = db.kv.config().context("Cannot find db config")?;
    db.kv.set_config(DBConfig {
        version: VERSION,
        ..config
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::test::assert_migration_types_hash;
    use scale_info::meta_type;

    #[test]
    fn ensure_migration_types() {
        assert_migration_types_hash(
            "v2->v3",
            vec![
                meta_type::<DBConfig>(),
                meta_type::<v0::BlockSmallData>(),
                meta_type::<BlockSmallData>(),
            ],
            "6506461993fe4e74645148eb4af27aecfef09e5b4789b5b9936c86adab62a8ff",
        );
    }
}
