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

use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;
use ethexe_common::{
    Announce, HashOf, PromisePolicy, SimpleBlockData,
    db::{AnnounceStorageRO, LatestData, LatestDataStorageRO, OnChainStorageRO},
};
use ethexe_db::{
    Database, RocksDatabase,
    iterator::{BlockNode, DatabaseIterator},
    verifier::IntegrityVerifier,
    visitor::{self},
};
use ethexe_processor::{Processor, ProcessorConfig};
use indicatif::{ProgressBar, ProgressStyle};
use std::{collections::HashSet, path::PathBuf};

// TODO: #5142 database integrity check is too slow, needs parallelization or some kind of optimization
const PROGRESS_BAR_TEMPLATE: &str = "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({percent}%) ETA {eta_precise} {msg}";

/// Run checks on ethexe database, see more in [`super::Command::Check`].
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

    /// Enable logging verbosity (debug level by default), disables progress bar.
    #[arg(short, long)]
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
        if self.verbose {
            crate::enable_logging("debug")?;
        }

        if !self.computation_check && !self.integrity_check {
            self.computation_check = true;
            self.integrity_check = true;
        }

        let rocks_db = RocksDatabase::open(self.db).context("failed to open rocks database")?;
        let db = Database::from_one(&rocks_db);

        let latest_data = db
            .latest_data()
            .ok_or_else(|| anyhow!("no latest data found in db"))?;

        let checker = Checker {
            db,
            latest_data,
            progress_bar: !self.verbose,
            chunk_size: self.chunk_size,
            canonical_quarantine: self.canonical_quarantine,
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

#[derive(Clone)]
struct Checker {
    db: Database,
    latest_data: LatestData,
    progress_bar: bool,
    chunk_size: usize,
    canonical_quarantine: u8,
}

impl Checker {
    async fn integrity_check(&self) -> Result<()> {
        let db = &self.db;
        let bottom = self.latest_data.start_block_hash;
        let head = self.latest_data.synced_block.hash;

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

    async fn computation_check(&self) -> Result<()> {
        let db = &self.db;
        let bottom = self.latest_data.start_announce_hash;
        let head = self.latest_data.computed_announce_hash;
        let progress_bar = self.progress_bar;
        let chunk_size = self.chunk_size;
        let canonical_quarantine = self.canonical_quarantine;

        let bottom_block = announce_block(db, bottom)?;
        let head_block = announce_block(db, head)?;
        println!(
            "📋 Starting computation check from announce {bottom} in {bottom_block} to announce {head} in {head_block}"
        );

        let pb = if progress_bar {
            let total_blocks = announce_block(db, head)?
                .header
                .height
                .checked_sub(announce_block(db, bottom)?.header.height)
                .ok_or_else(|| anyhow!("Incorrect announces range"))?;
            let bar_style = ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
                .unwrap()
                .progress_chars("=>-");
            let pb = ProgressBar::new(total_blocks as u64);
            pb.set_style(bar_style);
            Some(pb)
        } else {
            None
        };

        let processor = Processor::with_config(ProcessorConfig { chunk_size }, db.clone(), None)
            .context("failed to create processor")?;

        // Iterate back: from `head` announce to `bottom` announce
        let mut announce_hash = head;
        while announce_hash != bottom {
            let announce = db.announce(announce_hash).ok_or_else(|| {
                anyhow!("announce {announce_hash} in computed chain not found in db")
            })?;
            let announce_parent_hash = announce.parent;

            let mut processor = processor.clone().overlaid();
            let executable = ethexe_compute::prepare_executable_for_announce(
                db,
                announce,
                PromisePolicy::Disabled,
                canonical_quarantine,
            )
            .context("Unable to preparing announce data for execution")?;
            let res = processor
                .as_mut()
                .process_programs(executable)
                .await
                .context("failed to re-compute announce")?;

            let states = db.announce_program_states(announce_hash).ok_or_else(|| {
                anyhow!("program states for announce {announce_hash:?} not found in db",)
            })?;

            let outcome = db
                .announce_outcome(announce_hash)
                .ok_or_else(|| anyhow!("announce outcome {announce_hash:?} not found in db",))?;

            let schedule = db.announce_schedule(announce_hash).ok_or_else(|| {
                anyhow!("schedule for announce {announce_hash:?} not found in db",)
            })?;

            ensure!(
                states == res.states,
                "announce {announce_hash:?} final program states mismatch",
            );

            ensure!(
                outcome == res.transitions,
                "announce {announce_hash:?} state transitions mismatch",
            );

            ensure!(
                schedule == res.schedule,
                "announce {announce_hash:?} schedule mismatch",
            );

            if let Some(ref pb) = pb {
                pb.inc(1);
            }

            announce_hash = announce_parent_hash;
        }

        Ok(())
    }
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
