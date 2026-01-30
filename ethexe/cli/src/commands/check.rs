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

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ethexe_common::{
    Announce, HashOf, ProgramStates, Schedule, SimpleBlockData,
    db::{AnnounceStorageRO, LatestData, LatestDataStorageRO, OnChainStorageRO},
    gear::StateTransition,
};
use ethexe_compute::{ComputeConfig, ComputeSubService};
use ethexe_db::{
    Database, RocksDatabase,
    iterator::{BlockNode, DatabaseIterator},
    verifier::IntegrityVerifier,
    visitor::{self},
};
use ethexe_processor::{Processor, ProcessorConfig};
use indicatif::{ProgressBar, ProgressStyle};
use sp_core::H256;
use std::{collections::HashSet, path::PathBuf};

const PROGRESS_BAR_TEMPLATE: &str = "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({percent}%) ETA {eta_precise} {msg}";

/// Submit a transaction.
#[derive(Debug, Parser)]
pub struct CheckCommand {
    /// Path to database directory (including router addr subdirectory).
    #[arg(long)]
    pub db: PathBuf,

    #[arg(long, default_value = "2")]
    pub chunk_size: usize,

    #[arg(long, default_value = "4")]
    pub canonical_quarantine: u8,

    /// Perform computations of announces, by default from start announce to latest computed announce.
    #[arg(long, alias = "compute")]
    pub computation_check: bool,

    /// Perform integrity check of the database, by default from start block to latest prepared block.
    #[arg(long, alias = "integrity")]
    pub integrity_check: bool,

    /// Perform full check (computation + integrity).
    #[arg(long, alias = "full")]
    pub full_check: bool,

    /// Show progress bar.
    #[arg(long, alias = "pb", default_value = "true")]
    pub progress_bar: bool,

    pub verbose: bool,
}

impl CheckCommand {
    /// Execute the command.
    pub fn exec(self) -> Result<()> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(self.exec_inner())
    }

    async fn exec_inner(mut self) -> Result<()> {
        if self.full_check {
            self.computation_check = true;
            self.integrity_check = true;
        }

        let rocks_db = RocksDatabase::open(self.db).context("failed to open rocks database")?;
        let db = Database::from_one(&rocks_db);

        let LatestData {
            start_announce_hash,
            computed_announce_hash,
            start_block_hash,
            prepared_block_hash,
            ..
        } = db
            .latest_data()
            .ok_or_else(|| anyhow!("no latest data found in db"))?;

        if self.integrity_check {
            println!(
                "📋 Starting integrity check from block {start_block_hash} to {prepared_block_hash}"
            );
            integrity_check(&db, start_block_hash, prepared_block_hash)
                .await
                .context("integrity check failed")?;
        }

        if self.computation_check {
            println!(
                "📋 Starting computation check from announce {start_announce_hash} to {computed_announce_hash}"
            );
            computation_check(
                &db,
                self.chunk_size,
                self.canonical_quarantine,
                start_announce_hash,
                computed_announce_hash,
            )
            .await
            .context("computation check failed")?;
        }

        Ok(())
    }
}

async fn integrity_check(db: &Database, from: H256, to: H256) -> Result<()> {
    let from = db
        .block_header(from)
        .map(|header| SimpleBlockData { hash: from, header })
        .ok_or_else(|| anyhow!("start block not found in db"))?;
    let to = db
        .block_header(to)
        .map(|header| SimpleBlockData { hash: to, header })
        .ok_or_else(|| anyhow!("end block not found in db"))?;

    let total_blocks = to
        .header
        .height
        .checked_sub(from.header.height)
        .ok_or_else(|| anyhow!("Incorrect blocks range"))?;

    let bar_style = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
        .unwrap()
        .progress_chars("=>-");

    let pb = ProgressBar::new(total_blocks as u64);
    pb.set_style(bar_style);

    let mut verifier = IntegrityVerifier::new(db.clone());

    // Iterate back: from `to` block to `from` block
    let mut block = to;
    let mut visited_nodes = HashSet::new();
    while block.hash != from.hash {
        DatabaseIterator::new_skip_nodes(
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

        pb.inc(1);
    }

    if !verifier.errors().is_empty() {
        return Err(anyhow!(
            "Integrity check errors found: {:?}",
            verifier.errors()
        ));
    }

    Ok(())
}

async fn computation_check(
    db: &Database,
    chunk_size: usize,
    canonical_quarantine: u8,
    from: HashOf<Announce>,
    to: HashOf<Announce>,
) -> Result<()> {
    let total_blocks = announce_block(&db, to)?
        .header
        .height
        .checked_sub(announce_block(&db, from)?.header.height)
        .ok_or_else(|| anyhow!("Incorrect announces range"))?;

    let bar_style = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
        .unwrap()
        .progress_chars("=>-");

    let pb = ProgressBar::new(total_blocks as u64);
    pb.set_style(bar_style);

    let mut processor = Processor::with_config(ProcessorConfig { chunk_size }, db.clone())
        .context("failed to create processor")?;

    let compute_config = ComputeConfig::new(canonical_quarantine);

    // Iterate back: from `to` announce to `from` announce
    let mut announce_hash = to;
    while announce_hash != from {
        let announce = db
            .announce(announce_hash)
            .ok_or_else(|| anyhow!("announce {announce_hash} in computed chain not found in db"))?;
        let announce_parent_hash = announce.parent;

        let overlaid_db = unsafe { db.clone().overlaid() };
        processor.change_db(overlaid_db.clone());
        let _result = ComputeSubService::compute_one(
            &overlaid_db,
            &mut processor,
            announce_hash,
            announce,
            compute_config,
        )
        .await
        .context("failed to re-compute announce")?;

        let computed_result = announce_execution_result_from_db(&overlaid_db, announce_hash)
            .context("failed to get announce execution result from overlaid db")?;

        let db_result = announce_execution_result_from_db(&db, announce_hash)
            .context("failed to get announce execution result from db")?;

        if computed_result != db_result {
            return Err(anyhow!("announce {announce_hash} execution mismatch",));
        }

        pb.inc(1);

        announce_hash = announce_parent_hash;
    }

    Ok(())
}

fn announce_execution_result_from_db(
    db: &Database,
    announce_hash: HashOf<Announce>,
) -> Result<(ProgramStates, Vec<StateTransition>, Schedule)> {
    let states = db.announce_program_states(announce_hash).ok_or_else(|| {
        anyhow!(
            "program states for announce {:?} not found in db",
            announce_hash
        )
    })?;

    let outcome = db
        .announce_outcome(announce_hash)
        .ok_or_else(|| anyhow!("announce outcome {:?} not found in db", announce_hash))?;

    let schedule = db
        .announce_schedule(announce_hash)
        .ok_or_else(|| anyhow!("schedule for announce {:?} not found in db", announce_hash))?;

    Ok((states, outcome, schedule))
}

fn announce_block(db: &Database, announce_hash: HashOf<Announce>) -> Result<SimpleBlockData> {
    let announce = db
        .announce(announce_hash)
        .ok_or_else(|| anyhow!("announce {announce_hash} not found in db",))?;

    db.block_header(announce.block_hash)
        .ok_or_else(|| anyhow!("block header not found for block {}", announce.block_hash))
        .map(|header| SimpleBlockData {
            hash: announce.block_hash,
            header,
        })
}
