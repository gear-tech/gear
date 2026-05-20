// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Implementation of the `ethexe check` command.

use crate::params::{MergeParams, Params};
use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;
use ethexe_common::{
    SimpleBlockData,
    db::{DBGlobals, GlobalsStorageRO, MbStorageRO, OnChainStorageRO},
};
use ethexe_compute::prepare_executable_for_mb;
use ethexe_db::{
    Database, InitConfig, RawDatabase, RocksDatabase,
    iterator::{BlockNode, DatabaseIterator},
    verifier::IntegrityVerifier,
    visitor::{self},
};
use ethexe_processor::{Processor, ProcessorConfig};
use ethexe_runtime_common::FinalizedBlockTransitions;
use gprimitives::H256;
use indicatif::{ProgressBar, ProgressStyle};
use std::{collections::HashSet, path::PathBuf};

// TODO: #5142 database integrity check is too slow, needs parallelization or some kind of optimization
const PROGRESS_BAR_TEMPLATE: &str = "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({percent}%) ETA {eta_precise} {msg}";

/// Run checks on ethexe database, see more in [`super::Command::Check`].
#[derive(Debug, Parser)]
pub struct CheckCommand {
    /// CLI parameters to be merged with file ones before execution.
    #[clap(flatten)]
    pub params: Params,

    /// Override database location.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Re-execute every persisted MB through the processor and assert the
    /// cached outcome / states / schedule match the fresh computation.
    #[arg(long, alias = "compute")]
    pub computation_check: bool,

    /// Chunk size passed to the re-execution [`Processor`]. Controls how
    /// many programs the runtime works on per batch.
    #[arg(long, default_value = "2")]
    pub chunk_size: usize,

    /// Perform integrity check of the database, by default from start block to latest prepared block.
    #[arg(long, alias = "integrity")]
    pub integrity_check: bool,

    /// Perform migrations before checking the database.
    #[arg(long)]
    pub migrate: bool,

    /// Enable logging verbosity (debug level by default), disables progress bar.
    #[arg(short, long)]
    pub verbose: bool,
}

impl CheckCommand {
    /// Merges command-line options over file-backed parameters.
    pub fn with_params(mut self, params: Params) -> Self {
        self.params = self.params.merge(params);
        self
    }

    /// Execute the command.
    pub fn exec(self) -> Result<()> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(self.exec_inner())
    }

    async fn exec_inner(mut self) -> Result<()> {
        if self.verbose {
            crate::enable_logging("debug")?;
        }

        if !self.computation_check && !self.integrity_check {
            self.computation_check = true;
            self.integrity_check = true;
        }

        let rocks_db = RocksDatabase::open(
            self.db
                .or_else(|| self.params.node.as_ref().map(|node| node.db_dir()))
                .context("missing database path")?,
        )
        .context("failed to open rocks database")?;
        let raw_db = RawDatabase::from_one(&rocks_db);

        let db = if self.migrate {
            let ethereum_config = self
                .params
                .ethereum
                .context("missing Ethereum-related configuration")?
                .into_config()?;

            ethexe_db::initialize_db(
                InitConfig {
                    ethereum_rpc: ethereum_config.rpc.clone(),
                    router_address: ethereum_config.router_address,
                    slot_duration_secs: ethereum_config.block_time.as_secs(),
                    genesis_initializer: None,
                },
                raw_db.overlaid(),
            )
            .await?
        } else {
            Database::try_from_raw(raw_db)?
        };

        let globals = db.globals().clone();

        // Honor the node-level `chunk-processing-threads` from the
        // shared NodeParams so `ethexe check` lines up with the
        // operator's `ethexe run` configuration. The `--chunk-size`
        // CLI flag stays as an explicit override for one-off runs.
        let node_params = self.params.node.unwrap_or_default();
        let chunk_size = node_params
            .chunk_processing_threads
            .map(|n| n.get())
            .unwrap_or(self.chunk_size);
        let checker = Checker {
            db,
            globals,
            progress_bar: !self.verbose,
            chunk_size,
        };

        if self.integrity_check {
            checker
                .integrity_check()
                .await
                .context("integrity check failed")?;
        }

        if self.computation_check {
            checker
                .computation_check()
                .await
                .context("computation check failed")?;
        }

        Ok(())
    }
}

/// Shared state for the two database verification passes.
#[derive(Clone)]
struct Checker {
    db: Database,
    globals: DBGlobals,
    progress_bar: bool,
    chunk_size: usize,
}

impl Checker {
    /// Traverses the persisted block DAG and validates referential integrity.
    async fn integrity_check(&self) -> Result<()> {
        let db = &self.db;
        let bottom = self.globals.start_block_hash;
        let head = self.globals.latest_synced_eb.hash;

        let bottom = db
            .block_header(bottom)
            .map(|header| SimpleBlockData {
                hash: bottom,
                header,
            })
            .ok_or_else(|| anyhow!("start block not found in db"))?;
        let head = db
            .block_header(head)
            .map(|header| SimpleBlockData { hash: head, header })
            .ok_or_else(|| anyhow!("end block not found in db"))?;

        println!("📋 Starting integrity check from block {bottom} to {head}");

        let pb = if self.progress_bar {
            let total_blocks = head
                .header
                .height
                .checked_sub(bottom.header.height)
                .ok_or_else(|| anyhow!("Incorrect blocks range"))?;
            let bar_style = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
                .unwrap()
                .progress_chars("=>-");
            let pb = ProgressBar::new(total_blocks as u64);
            pb.set_style(bar_style);
            Some(pb)
        } else {
            None
        };

        let mut verifier = IntegrityVerifier::new(db.clone());

        // Iterate back: from `head` block to `bottom` block
        let mut block = head;
        let mut visited_nodes = HashSet::new();
        while block.hash != bottom.hash {
            // TODO: #5143 impl DST iterator to avoid using `visited_nodes` here
            DatabaseIterator::with_skip_nodes(
                &db,
                BlockNode { block: block.hash },
                visited_nodes.clone(),
            )
            .for_each(|node| {
                visited_nodes.insert(ethexe_db::iterator::node_hash(&node));
                visitor::visit_node(&mut verifier, node);
            });

            let parent_hash = block.header.parent_hash;
            block = db
                .block_header(parent_hash)
                .map(|header| SimpleBlockData {
                    hash: parent_hash,
                    header,
                })
                .ok_or_else(|| anyhow!("block header not found for block {parent_hash}"))?;

            if let Some(pb) = pb.as_ref() {
                pb.inc(1);
            };
        }

        let errors = verifier.into_errors();
        if !errors.is_empty() {
            return Err(anyhow!("Integrity check errors found: {errors:?}",));
        }

        Ok(())
    }

    /// Walks the MB chain back from `globals.latest_finalized_mb_hash`,
    /// re-executes each MB through a fresh [`Processor`] (using the
    /// same `ExecutableData` the live `compute_mb` pipeline assembles),
    /// and asserts the fresh outputs match the cached
    /// `mb_program_states` / `mb_outcome` / `mb_schedule` records.
    ///
    /// Each MB runs against an overlaid DB so writes from the
    /// re-execution don't pollute the on-disk state.
    async fn computation_check(&self) -> Result<()> {
        let head = self.globals.latest_finalized_mb_hash;
        if head.is_zero() {
            println!("📋 No finalized MB yet — nothing to verify");
            return Ok(());
        }

        let db = &self.db;

        let head_compact = db
            .mb_compact_block(head)
            .ok_or_else(|| anyhow!("latest_finalized_mb_hash {head} not in CompactMb store"))?;

        println!(
            "📋 Starting computation check from MB {head} (height {})",
            head_compact.height
        );

        let pb = if self.progress_bar {
            let bar_style = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
                .unwrap()
                .progress_chars("=>-");
            let pb = ProgressBar::new(head_compact.height + 1);
            pb.set_style(bar_style);
            Some(pb)
        } else {
            None
        };

        let processor = Processor::with_config(
            ProcessorConfig {
                chunk_size: self.chunk_size,
            },
            db.clone(),
        )
        .context("failed to create processor")?;

        let mut current_mb = head;
        loop {
            let current_compact_mb = db
                .mb_compact_block(current_mb)
                .ok_or_else(|| anyhow!("CompactMb missing for MB {current_mb}"))?;
            let height = current_compact_mb.height;
            let meta = db.mb_meta(current_mb);
            ensure!(
                meta.computed,
                "MB {current_mb} (height {height}) has not been computed",
            );

            let expected_states = db
                .mb_program_states(current_mb)
                .ok_or_else(|| anyhow!("program states missing for MB {current_mb}"))?;
            let expected_outcome = db
                .mb_outcome(current_mb)
                .ok_or_else(|| anyhow!("outcome missing for MB {current_mb}"))?;
            let expected_schedule = db
                .mb_schedule(current_mb)
                .ok_or_else(|| anyhow!("schedule missing for MB {current_mb}"))?;

            let executable = prepare_executable_for_mb(db, current_mb, current_compact_mb)
                .with_context(|| {
                    format!("failed to prepare executable data for MB {current_mb}")
                })?;

            // Overlaid DB so re-execution doesn't mutate persisted state.
            let mut overlay = processor.clone().overlaid();
            let FinalizedBlockTransitions {
                transitions,
                states,
                schedule,
                program_creations: _,
            } = overlay
                .as_mut()
                .process_programs(executable, None)
                .await
                .with_context(|| format!("failed to re-compute MB {current_mb}"))?;

            ensure!(
                states == expected_states,
                "MB {current_mb} (height {height}) program states mismatch",
            );
            ensure!(
                transitions == expected_outcome,
                "MB {current_mb} (height {height}) outcome mismatch",
            );
            ensure!(
                schedule == expected_schedule,
                "MB {current_mb} (height {height}) schedule mismatch",
            );

            if let Some(pb) = pb.as_ref() {
                pb.inc(1);
            };

            if current_compact_mb.parent == H256::zero() {
                break;
            }
            current_mb = current_compact_mb.parent;
        }

        Ok(())
    }
}
