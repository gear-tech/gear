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

use crate::{InitConfig, LATEST_VERSION, MIGRATIONS};
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, ensure};
use ethexe_common::{
    Announce, BlockHeader, HashOf, ProtocolTimelines, SimpleBlockData,
    db::{ComputedAnnounceData, PreparedBlockData},
    gear::{GenesisBlockInfo, Timelines},
};
use ethexe_db::DatabaseRef;
use ethexe_ethereum::router::RouterQuery;
use gprimitives::H256;

pub async fn initialize_db(config: InitConfig, db: DatabaseRef<'_, '_>) -> Result<()> {
    log::info!("Initializing database to version {LATEST_VERSION}...");

    if db.kv.is_empty() {
        log::info!(
            "KV database is empty, start base initialization to version {LATEST_VERSION}..."
        );
        initialize_empty_db(config, db).await?;
    } else {
        let db_version = db.kv.version()?;

        ensure!(
            db_version != Some(0),
            "Database at version 0 must not have config, but we found it. Consider to clean up database"
        );
        let db_version = db_version.unwrap_or(0);

        ensure!(
            db_version <= LATEST_VERSION,
            "Cannot initialize database to version {LATEST_VERSION} from version {}",
            db_version
        );

        log::info!("Database has version {db_version}");

        for (from_version, &migration) in MIGRATIONS.iter().enumerate() {
            if from_version >= db_version as usize {
                Box::into_pin(migration.migrate(&config, &db)).await?;
            }
        }

        validate_db(config, db).await?;
    }

    log::info!("Database initialized to version {LATEST_VERSION}");

    Ok(())
}

async fn validate_db(config: InitConfig, db: DatabaseRef<'_, '_>) -> Result<()> {
    let db_config = db.kv.config()?;
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

    Ok(())
}

pub async fn initialize_empty_db<'a, 'b>(
    config: InitConfig,
    db: DatabaseRef<'a, 'b>,
) -> Result<()> {
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
        version: LATEST_VERSION,
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
