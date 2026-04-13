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

use std::collections::BTreeMap;

use super::{InitConfig, LATEST_VERSION, MIGRATIONS, OLDEST_SUPPORTED_VERSION};
use crate::{Database, RawDatabase, dump::StateDump, migrations::GenesisInitializer};
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, bail, ensure};
use ethexe_common::{
    Announce, BlockHeader, HashOf, ProgramStates, ProtocolTimelines, Schedule, SimpleBlockData,
    StateHashWithQueueSize,
    db::{CodesStorageRO, CodesStorageRW, ComputedAnnounceData, PreparedBlockData},
    gear::{GenesisBlockInfo, Timelines},
};
use ethexe_ethereum::router::RouterQuery;
use ethexe_runtime_common::{RUNTIME_ID, ScheduleRestorer, state::Storage};
use futures::{TryStreamExt, stream::FuturesUnordered};
use gprimitives::{CodeId, H256};

pub async fn initialize_db(config: InitConfig, db: RawDatabase) -> Result<Database> {
    log::info!("Initializing database to version {LATEST_VERSION}...");

    if db.kv.is_empty() {
        log::info!(
            "KV database is empty, start base initialization to version {LATEST_VERSION}..."
        );
        initialize_empty_db(config, &db).await?;
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

        #[allow(clippy::absurd_extreme_comparisons)]
        if db_version < OLDEST_SUPPORTED_VERSION {
            log::info!(
                "The oldest supported database version is {}",
                OLDEST_SUPPORTED_VERSION
            );
            bail!(
                "Database version is too old: expected at least {}, found {}",
                OLDEST_SUPPORTED_VERSION,
                db_version
            );
        }

        for (i, &migration) in MIGRATIONS.iter().enumerate() {
            let from_version = i as u32 + OLDEST_SUPPORTED_VERSION;

            if from_version >= db_version {
                log::info!(
                    "Migrating the database from version {} to version {}",
                    from_version,
                    from_version + 1
                );

                migration.migrate(&config, &db).await?;

                let version_after_migration = db
                    .kv
                    .version()
                    .and_then(|v| v.context("Config not found"))
                    .context("Cannot retrieve database version after migration")?;
                ensure!(
                    version_after_migration == from_version + 1,
                    "Expected database version {}, but found {}",
                    from_version + 1,
                    version_after_migration
                );

                log::info!(
                    "Migration from version {} to version {} completed",
                    from_version,
                    from_version + 1
                );
            }
        }

        validate_db(config, &db).await?;
    }

    log::info!("Database initialized to version {LATEST_VERSION}");

    Database::try_from_raw(db).context("Failed to create database from raw after initialization")
}

async fn validate_db(config: InitConfig, db: &RawDatabase) -> Result<()> {
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

pub async fn initialize_empty_db(config: InitConfig, db: &RawDatabase) -> Result<()> {
    let provider = RootProvider::connect(&config.ethereum_rpc).await?;
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

    let genesis_announce = Announce {
        block_hash: genesis_block.hash,
        parent: HashOf::zero(),
        gas_allowance: None,
        injected_transactions: vec![],
    };

    let (program_states, schedule) = if let Some(initializer) = config.genesis_initializer {
        genesis_data_initialization(initializer, db, genesis_block).await?
    } else {
        (Default::default(), Default::default())
    };

    let genesis_announce_hash = ethexe_common::setup_announce_in_db(
        &db,
        ComputedAnnounceData {
            announce: genesis_announce,
            program_states,
            schedule,
            outcome: Default::default(),
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

async fn genesis_data_initialization(
    mut initializer: Box<dyn GenesisInitializer>,
    db: &RawDatabase,
    genesis_block: SimpleBlockData,
) -> Result<(ProgramStates, Schedule)> {
    log::info!("Start genesis {genesis_block} data initialization...");

    let StateDump {
        announce_hash: _,
        block_hash,
        codes,
        programs,
        blobs,
    } = initializer.get_genesis_data()?;

    ensure!(
        block_hash == genesis_block.hash,
        "Genesis data block hash {block_hash} does not match the actual genesis block hash {}",
        genesis_block.hash
    );

    log::info!(
        "Genesis data contains {} codes, {} programs, {} blobs",
        codes.len(),
        programs.len(),
        blobs.len()
    );

    let mut code_bytes = BTreeMap::<CodeId, Vec<u8>>::new();
    for blob in blobs {
        let hash = db.cas.write(&blob);
        let code_id = CodeId::from(hash.0);
        if codes.contains(&code_id) {
            code_bytes.insert(code_id, blob);
        };
    }

    ensure!(
        code_bytes.len() == codes.len(),
        "Genesis data contains {} valid codes, but only {} code blobs were provided",
        codes.len(),
        code_bytes.len()
    );

    let code_processing_futures = FuturesUnordered::new();
    for (code_id, code) in code_bytes {
        let process = initializer.process_code(code_id, code);
        let db_clone = db.clone();
        code_processing_futures.push(async move {
            let Some((instrumented_code, code_metadata)) = process.await? else {
                bail!("Genesis data contains invalid code {code_id}");
            };

            // Should not happen because we checked that code_bytes.len() == codes.len(),
            // so all codes must be present in the database.
            ensure!(
                db_clone.original_code_exists(code_id),
                "code {code_id} must be already present in database",
            );

            db_clone.set_code_metadata(code_id, code_metadata);
            db_clone.set_instrumented_code(RUNTIME_ID, code_id, instrumented_code);
            db_clone.set_code_valid(code_id, true);

            Ok::<_, anyhow::Error>(())
        });
    }

    let _results = code_processing_futures
        .try_collect::<Vec<_>>()
        .await
        .context("Failed to process genesis code")?;

    let mut program_states = ProgramStates::new();
    for (program_id, (code_id, state_hash)) in programs {
        db.set_program_code_id(program_id, code_id);
        let program_state = db
            .cas
            .program_state(state_hash)
            .context("Incorrect genesis data: program state blob must be present")?;
        program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: program_state.canonical_queue.cached_queue_size,
                injected_queue_size: program_state.injected_queue.cached_queue_size,
            },
        );
    }

    let schedule =
        ScheduleRestorer::from_storage(&db.cas, &program_states, genesis_block.header.height)?
            .restore();
    log::info!(
        "Genesis schedule restored, tasks amount {}",
        schedule.iter().flat_map(|(_, tasks)| tasks.iter()).count()
    );

    log::info!("Genesis data initialization completed");

    Ok((program_states, schedule))
}
