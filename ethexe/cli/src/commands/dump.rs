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

use crate::params::{MergeParams, Params};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ethexe_common::{
    Announce, HashOf,
    db::{BlockMetaStorageRO, GlobalsStorageRO},
};
use ethexe_db::{Database, RawDatabase, RocksDatabase, dump::StateDump};
use gprimitives::H256;
use std::path::{Path, PathBuf};

/// State dump operations for re-genesis.
#[derive(Debug, Parser)]
pub struct DumpCommand {
    #[clap(flatten)]
    pub params: Params,

    /// Override database location.
    #[arg(long)]
    pub db: Option<PathBuf>,

    #[command(subcommand)]
    pub command: DumpSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DumpSubcommand {
    /// Create a state dump from the database and write it to a file.
    /// Use `.blob` extension for binary format or `.json` for JSON format.
    /// If --announce-hash is not provided, uses the latest committed announce.
    Create {
        /// Announce hash (hex-encoded, with or without 0x prefix).
        /// If omitted, the latest committed announce is used.
        #[arg(long)]
        announce_hash: Option<String>,

        /// Output file path (.blob for binary, .json for JSON).
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Read a state dump from a .blob file and print it as JSON to stdout.
    Json {
        /// Dump file path (.blob).
        #[arg(long, short)]
        file: PathBuf,
    },
}

impl DumpCommand {
    pub fn with_params(mut self, params: Params) -> Self {
        self.params = self.params.merge(params);
        self
    }

    pub fn exec(self) -> Result<()> {
        match &self.command {
            DumpSubcommand::Create {
                announce_hash,
                output,
            } => self.exec_create(announce_hash.as_deref(), output),
            DumpSubcommand::Json { file } => Self::exec_json(file),
        }
    }

    fn exec_create(&self, announce_hash_str: Option<&str>, output: &Path) -> Result<()> {
        crate::enable_logging("info")?;

        let rocks_db = RocksDatabase::open(
            self.db
                .clone()
                .or_else(|| self.params.node.as_ref().map(|node| node.db_dir()))
                .context("missing database path")?,
        )
        .context("failed to open database")?;

        let raw_db = RawDatabase::from_one(&rocks_db);
        let db = Database::try_from_raw(raw_db)?;

        let announce_hash = match announce_hash_str {
            Some(s) => parse_announce_hash(s)?,
            None => {
                let latest_prepared_block = db.globals().latest_prepared_block_hash;
                let block_meta = db.block_meta(latest_prepared_block);
                let announce_hash = block_meta
                    .last_committed_announce
                    .context("no committed announce found for latest prepared block")?;

                log::info!(
                    "No announce hash provided, using latest committed announce: {announce_hash}"
                );

                announce_hash
            }
        };

        log::info!("Collecting state dump for announce {announce_hash:?}...");
        let dump = StateDump::collect_from_storage(&db, announce_hash)?;

        log::info!(
            "Dump collected: {} codes, {} programs, {} blobs",
            dump.codes.len(),
            dump.programs.len(),
            dump.blobs.len(),
        );

        dump.write_to_file(output)?;
        log::info!("Dump written to {}", output.display());
        Ok(())
    }

    fn exec_json(file: &Path) -> Result<()> {
        let dump = StateDump::read_from_blob(file).context("failed to read .blob dump file")?;
        let json = serde_json::to_string_pretty(&dump)?;
        println!("{json}");
        Ok(())
    }
}

fn parse_announce_hash(s: &str) -> Result<HashOf<Announce>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).context("invalid hex for announce hash")?;
    anyhow::ensure!(bytes.len() == 32, "announce hash must be 32 bytes");
    let h256 = H256::from_slice(&bytes);
    // SAFETY: constructing HashOf from a user-provided hash for DB lookup.
    Ok(unsafe { HashOf::new(h256) })
}
