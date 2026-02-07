use super::{DB_VERSION_0, DB_VERSION_1};
use crate::init::InitConfig;
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, anyhow, bail};
use ethexe_common::{
    Announce, BlockHeader, HashOf, ProtocolTimelines, SimpleBlockData,
    db::{ComputedAnnounceData, PreparedBlockData},
    gear::Timelines,
};
use ethexe_db::DatabaseRef;
use ethexe_ethereum::router::RouterQuery;
use gprimitives::H256;
use parity_scale_codec::Decode;

pub async fn initialize_db<'a, 'b>(config: InitConfig, db: DatabaseRef<'a, 'b>) -> Result<()> {
    if ethexe_db::VERSION != DB_VERSION_1 {
        bail!(
            "Cannot initializing database to version 1, because current impl version is {}",
            ethexe_db::VERSION
        );
    }

    log::info!("Initializing database to version 1...");

    let db_config = db
        .config()
        .context("Config key is occupied but cannot be decoded")?;

    if let Some(db_config) = db_config {
        log::info!("Database config found, version {}", db_config.version);

        if db_config.version == DB_VERSION_1 {
            // if version matches, then we can use the existing database
            log::info!("Database is already initialized to version 1");

            // +_+_+ check chain id and router address are the same

            return Ok(());
        }

        if db_config.version != DB_VERSION_0 {
            bail!(
                "Cannot initialize database to version 1 from version {}",
                db_config.version
            );
        } else {
            bail!(
                "Database at version 0 must not have config, but we found it. Consider to clean up database files."
            );
        }
    } else {
        // We do not care about CAS emptiness,
        // because in version 1 we have the same CAS layout as in version 0
        if db.kv.is_empty() {
            log::info!("KV database is empty, start base initialization to version 1");
            initialize_empty_db(config, db).await?;
            log::info!("Database initialized to version 1");
            return Ok(());
        }
    }

    log::info!("Database at version 0 detected, start migration to version 1...");
    migration_from_version0(config, db).await?;

    Ok(())
}

pub async fn initialize_empty_db<'a, 'b>(
    config: InitConfig,
    db: DatabaseRef<'a, 'b>,
) -> Result<()> {
    if ethexe_db::VERSION != DB_VERSION_1 {
        bail!(
            "Cannot initializing database to version 1, because current impl version is {}",
            ethexe_db::VERSION
        );
    }

    let provider = RootProvider::connect(&config.ethereum_rpc).await.unwrap();

    let chain_id = provider.get_chain_id().await?;

    let storage_view = RouterQuery::from_provider(config.router_address, provider)
        .storage_view_at(alloy::eips::BlockId::latest())
        .await
        .context("Empty db init, failed read router data")?;

    let (genesis_block_hash, genesis_block_height, genesis_block_timestamp) =
        storage_view.genesis_block_info();

    let genesis_block = SimpleBlockData {
        hash: genesis_block_hash,
        header: BlockHeader {
            // genesis block header is not important in any way for ethexe
            parent_hash: H256::zero(),
            height: genesis_block_height,
            timestamp: genesis_block_timestamp,
        },
    };

    let genesis_announce_hash = ethexe_common::setup_announce_in_db(
        &db,
        ComputedAnnounceData {
            announce: Announce {
                block_hash: genesis_block.hash,
                parent: HashOf::zero(),
                gas_allowance: None,
                injected_transactions: vec![],
            },
            program_states: Default::default(),
            outcome: Default::default(),
            schedule: Default::default(),
        },
    );

    ethexe_common::setup_block_in_db(
        &db,
        genesis_block.hash,
        PreparedBlockData {
            header: genesis_block.header,
            events: Default::default(),
            codes_queue: Default::default(),
            announces: [genesis_announce_hash].into(),
            last_committed_batch: Default::default(),
            last_committed_announce: HashOf::zero(),
            latest_era_with_committed_validators: 0,
        },
    );

    let timelines: Timelines = storage_view.timelines.into();

    let db_config = ethexe_common::db::DBConfig {
        version: DB_VERSION_1,
        chain_id,
        router_address: config.router_address,
        timelines: ProtocolTimelines {
            genesis_ts: genesis_block.header.timestamp,
            era: timelines.era,
            election: timelines.election,
            slot: config.slot_duration_secs,
        },
        genesis_block_hash,
        genesis_announce_hash,
    };

    // NOTE: start block and announce could be changed later by fast-sync
    let globals = ethexe_common::db::DBGlobals {
        start_block: genesis_block.hash,
        start_announce_hash: genesis_announce_hash,
        latest_synced_block: genesis_block,
        latest_prepared_block_hash: genesis_block.hash,
        latest_computed_announce_hash: genesis_announce_hash,
    };

    db.set_globals(globals);
    db.set_config(db_config);

    Ok(())
}

pub async fn migration_from_version0<'a, 'b>(
    config: InitConfig,
    db: DatabaseRef<'a, 'b>,
) -> Result<()> {
    if ethexe_db::VERSION != DB_VERSION_1 {
        bail!(
            "Cannot migrate database to version 1 from version 0, because current impl version is {}",
            ethexe_db::VERSION
        );
    }

    // Changes from version 0 to version 1:
    // 1) LatestData is removed, and some fields are moved to DBGlobals
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.
    // 2) Timelines is moved to more common DBConfig.
    //    DB keys have the same prefix, but appends 8 zero bytes in the end.

    let provider: RootProvider = RootProvider::connect(&config.ethereum_rpc).await.unwrap();

    let chain_id = provider.get_chain_id().await?;

    let latest_data_key = H256::from_low_u64_be(14);
    let timelines_key = H256::from_low_u64_be(15);

    #[derive(Decode)]
    pub struct LatestData {
        pub synced_block: SimpleBlockData,
        pub prepared_block_hash: H256,
        pub computed_announce_hash: HashOf<Announce>,
        pub genesis_block_hash: H256,
        pub genesis_announce_hash: HashOf<Announce>,
        pub start_block_hash: H256,
        pub start_announce_hash: HashOf<Announce>,
    }

    let latest_data = db
        .kv
        .get(latest_data_key.as_bytes())
        .ok_or_else(|| anyhow!("latest data not found for db at version 0"))
        .map(|bytes| LatestData::decode(&mut bytes.as_slice()))?
        .context("failed to decode LatestData during migration")?;

    let globals = ethexe_common::db::DBGlobals {
        start_block: latest_data.start_block_hash,
        start_announce_hash: latest_data.start_announce_hash,
        latest_synced_block: latest_data.synced_block,
        latest_prepared_block_hash: latest_data.prepared_block_hash,
        latest_computed_announce_hash: latest_data.computed_announce_hash,
    };

    db.set_globals(globals);

    let timelines = db
        .kv
        .get(timelines_key.as_bytes())
        .ok_or_else(|| anyhow!("timelines not found for db at version 0"))
        .map(|bytes| ProtocolTimelines::decode(&mut bytes.as_slice()))?
        .context("failed to decode ProtocolTimelines during migration")?;

    let db_config = ethexe_common::db::DBConfig {
        version: DB_VERSION_1,
        chain_id,
        router_address: config.router_address,
        timelines,
        genesis_block_hash: latest_data.genesis_block_hash,
        genesis_announce_hash: latest_data.genesis_announce_hash,
    };

    db.set_config(db_config);

    Ok(())
}
