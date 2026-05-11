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

//! Implementation of the `ethexe check` command.

use crate::params::{MergeParams, Params};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ethexe_common::{
    SimpleBlockData,
    db::{DBGlobals, GlobalsStorageRO, OnChainStorageRO},
};
use ethexe_db::{
    Database, InitConfig, RawDatabase, RocksDatabase,
    iterator::{BlockNode, DatabaseIterator},
    verifier::IntegrityVerifier,
    visitor::{self},
};
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

    /// Re-execute every persisted MB and assert the cached outcome /
    /// states / schedule match. Currently disabled — not yet wired in.
    #[arg(long, alias = "compute")]
    pub computation_check: bool,

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

        let _node_params = self.params.node.unwrap_or_default();
        let checker = Checker {
            db,
            globals,
            progress_bar: !self.verbose,
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
}

impl Checker {
    /// Traverses the persisted block DAG and validates referential integrity.
    async fn integrity_check(&self) -> Result<()> {
        let db = &self.db;
        let bottom = self.globals.start_block_hash;
        let head = self.globals.latest_synced_block.hash;

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

    /// Re-runs every persisted MB and compares the cached outcome / states /
    /// schedule against fresh execution. Stubbed pending MB walk wiring.
    async fn computation_check(&self) -> Result<()> {
        // TODO: (+_+_+ append issue number) walk `globals.latest_finalized_mb_hash` back through
        // `CompactBlock.parent`, re-execute each MB through the
        // processor, and assert the persisted `mb_*` records match.
        println!("computation_check is currently a stub — MB walk not wired in yet");
        Ok(())
    }
}
