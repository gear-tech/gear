// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::collections::BTreeMap;

use super::{InitConfig, LATEST_VERSION, migrate};
use crate::{Database, RawDatabase, dump::StateDump, migrations::GenesisInitializer};
use alloy::providers::{Provider as _, RootProvider};
use anyhow::{Context as _, Result, bail, ensure};
use ethexe_common::{
    BlockHeader, ProgramStates, ProtocolTimelines, Schedule, SimpleBlockData,
    StateHashWithQueueSize,
    db::{CodesStorageRO, CodesStorageRW, CompactMb, MbStorageRW, PreparedBlockData},
    gear::{GenesisBlockInfo, Timelines},
    malachite::Transactions,
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
        let db_version = db.kv.version()?.context("Version not found")?;
        log::info!("Database has version {db_version}");

        if db_version != LATEST_VERSION {
            log::info!("Upgrading database from version {db_version} to {LATEST_VERSION}...");
            migrate(&config, &db)
                .await
                .context("Failed to migrate database")?;
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
        db_config.timelines.slot.get() == config.slot_duration_secs,
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

    let genesis_mb = if let Some(initializer) = config.genesis_initializer {
        let (mb_hash, program_states, schedule) =
            genesis_data_initialization(initializer, db, genesis_block).await?;
        // Seed MB rows so RPC reads (program_states / schedule / outcome)
        // resolve before the first post-genesis MB lands. The empty
        // Transactions blob is persisted in CAS so downstream walkers
        // (`prepare_executable_for_mb`, `ethexe check`) can resolve
        // `transactions_hash` without tripping on `H256::zero`.
        let transactions_hash = db.set_transactions(Transactions::default());
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: H256::zero(),
                height: 0,
                transactions_hash,
            },
        );
        db.set_mb_program_states(mb_hash, program_states);
        db.set_mb_schedule(mb_hash, schedule);
        db.set_mb_outcome(mb_hash, Vec::new());
        db.mutate_mb_meta(mb_hash, |m| {
            m.computed = true;
            m.last_advanced_eb = genesis_block.hash;
        });
        Some(mb_hash)
    } else {
        None
    };

    ethexe_common::setup_block_in_db(
        &db,
        genesis_block.hash,
        PreparedBlockData {
            header: genesis_block.header,
            events: Default::default(),
            codes_queue: Default::default(),
            last_committed_batch: Default::default(),
            last_committed_mb: H256::zero(),
            last_committed_eb: H256::zero(),
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
            era: timelines
                .era
                .try_into()
                .context("era duration must be non-zero")?,
            election: timelines.election,
            slot: config
                .slot_duration_secs
                .try_into()
                .context("slot duration must be non-zero")?,
        },
        genesis_block_hash: genesis.hash,
        max_validators: storage_view.maxValidators,
    };

    // NOTE: start block could be changed later by fast-sync
    let genesis_mb_hash = genesis_mb.unwrap_or(H256::zero());
    let globals = ethexe_common::db::DBGlobals {
        start_block_hash: genesis_block.hash,
        latest_synced_eb: genesis_block,
        latest_prepared_eb_hash: genesis_block.hash,
        latest_finalized_mb_hash: genesis_mb_hash,
        latest_computed_mb_hash: genesis_mb_hash,
    };

    db.kv.set_globals(globals);
    db.kv.set_config(db_config);

    Ok(())
}

async fn genesis_data_initialization(
    mut initializer: Box<dyn GenesisInitializer>,
    db: &RawDatabase,
    genesis_block: SimpleBlockData,
) -> Result<(H256, ProgramStates, Schedule)> {
    log::info!("Start genesis {genesis_block} data initialization...");

    let StateDump {
        mb_hash,
        block_hash,
        codes,
        programs,
        blobs,
    } = initializer.get_genesis_data()?;

    if block_hash != genesis_block.hash {
        log::warn!(
            "Genesis data block hash {block_hash} does not match the actual genesis block hash {}",
            genesis_block.hash
        );
    }

    log::info!(
        "Genesis data for MB {mb_hash} and block {block_hash} \
         contains {} codes, {} programs, {} blobs",
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

    Ok((mb_hash, program_states, schedule))
}
