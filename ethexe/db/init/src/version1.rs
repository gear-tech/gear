// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use super::{DB_VERSION_0, DB_VERSION_1, InitConfig};
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use ethexe_common::{
    Announce, BlockHeader, HashOf, ProtocolTimelines, SimpleBlockData,
    db::{ComputedAnnounceData, PreparedBlockData},
    gear::{GenesisBlockInfo, Timelines},
};
use ethexe_db::DatabaseRef;
use ethexe_ethereum::router::RouterQuery;
use gprimitives::H256;
use parity_scale_codec::Decode;

pub async fn initialize_db<'a, 'b>(config: InitConfig, db: DatabaseRef<'a, 'b>) -> Result<()> {
    if ethexe_db::VERSION != DB_VERSION_1 {
        bail!(
            "Cannot initializing database to version {DB_VERSION_1}, because current impl version is {}",
            ethexe_db::VERSION
        );
    }

    log::info!("Initializing database to version {DB_VERSION_1}...");

    let db_config = db.kv.config();

    if let Some(db_config) = db_config {
        let db_config = db_config.context("Database config is occupied but cannot be decoded")?;

        log::info!("Database config found, version {}", db_config.version);

        if db_config.version == DB_VERSION_1 {
            // if version matches, then we can use the existing database
            log::info!("Database is already initialized to version 1");

            let provider: RootProvider = RootProvider::connect(&config.ethereum_rpc).await?;
            let chain_id = provider.get_chain_id().await?;

            ensure!(
                db_config.chain_id == chain_id,
                "Database chain id {} does not match the provided ethereum rpc chain id {chain_id}",
                db_config.chain_id,
            );
            ensure!(
                db_config.router_address == config.router_address,
                "Database router address {:?} does not match the provided router address {:?}",
                db_config.router_address,
                config.router_address
            );
            ensure!(
                db_config.timelines.slot == config.slot_duration_secs,
                "Database slot duration {} does not match the provided slot duration {}",
                db_config.timelines.slot,
                config.slot_duration_secs
            );

            return Ok(());
        } else if db_config.version == DB_VERSION_0 {
            bail!(
                "Database at version {DB_VERSION_0} must not have config, but we found it.
                Consider to clean up database"
            );
        } else {
            bail!(
                "Cannot initialize database to version {DB_VERSION_1} from version {}",
                db_config.version
            );
        }
    } else if db.kv.is_empty() {
        // We do not care about CAS emptiness,
        // because in version 1 we have the same CAS layout as in version 0
        log::info!("KV database is empty, start base initialization to version {DB_VERSION_1}...");
        initialize_empty_db(config, db).await?;
    } else {
        log::info!(
            "Database at version {DB_VERSION_0} detected, start migration to version {DB_VERSION_1}..."
        );
        migration_from_version0(config, db).await?;
    }

    log::info!("Database initialized initialized to version {DB_VERSION_1}");

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

    let genesis: GenesisBlockInfo = storage_view.genesisBlock.into();

    let genesis_block = SimpleBlockData {
        hash: genesis.hash,
        header: BlockHeader {
            // genesis block header is not important in any way for ethexe
            parent_hash: H256::zero(),
            height: genesis.number,
            timestamp: genesis.timestamp,
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
        genesis_block_hash: genesis.hash,
        genesis_announce_hash,
    };

    // NOTE: start block and announce could be changed later by fast-sync
    let globals = ethexe_common::db::DBGlobals {
        start_block_hash: genesis_block.hash,
        start_announce_hash: genesis_announce_hash,
        latest_synced_block: genesis_block,
        latest_prepared_block_hash: genesis_block.hash,
        latest_computed_announce_hash: genesis_announce_hash,
    };

    db.kv.set_globals(globals);
    db.kv.set_config(db_config);

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
        synced_block: SimpleBlockData,
        prepared_block_hash: H256,
        computed_announce_hash: HashOf<Announce>,
        genesis_block_hash: H256,
        genesis_announce_hash: HashOf<Announce>,
        start_block_hash: H256,
        start_announce_hash: HashOf<Announce>,
    }

    #[derive(Decode)]
    pub struct ProtocolTimelinesV0 {
        pub genesis_ts: u64,
        pub era: u64,
        pub election: u64,
    }

    let latest_data = db
        .kv
        .get(latest_data_key.as_bytes())
        .ok_or_else(|| anyhow!("latest data not found for db at version {DB_VERSION_0}"))
        .map(|bytes| LatestData::decode(&mut bytes.as_slice()))?
        .context("failed to decode LatestData during migration")?;

    let globals = ethexe_common::db::DBGlobals {
        start_block_hash: latest_data.start_block_hash,
        start_announce_hash: latest_data.start_announce_hash,
        latest_synced_block: latest_data.synced_block,
        latest_prepared_block_hash: latest_data.prepared_block_hash,
        latest_computed_announce_hash: latest_data.computed_announce_hash,
    };

    db.kv.set_globals(globals);

    let timelines = db
        .kv
        .get(timelines_key.as_bytes())
        .ok_or_else(|| anyhow!("timelines not found for db at version 0"))
        .map(|bytes| ProtocolTimelinesV0::decode(&mut bytes.as_slice()))?
        .context("failed to decode ProtocolTimelines during migration")?;

    let db_config = ethexe_common::db::DBConfig {
        version: DB_VERSION_1,
        chain_id,
        router_address: config.router_address,
        timelines: ProtocolTimelines {
            genesis_ts: timelines.genesis_ts,
            era: timelines.era,
            election: timelines.election,
            slot: config.slot_duration_secs,
        },
        genesis_block_hash: latest_data.genesis_block_hash,
        genesis_announce_hash: latest_data.genesis_announce_hash,
    };

    db.kv.set_config(db_config);

    Ok(())
}
