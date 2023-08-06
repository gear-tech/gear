// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Replaying a block against the live chain state

use clap::{Parser, Subcommand};
use cmd::*;
use runtime_primitives::Block;
use sc_tracing::logging::LoggerBuilder;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::fmt::Debug;

pub(crate) const LOG_TARGET: &str = "gear_replay";

mod cmd;
mod parse;
mod util;

const VARA_SS58_PREFIX: u8 = 137;
const GEAR_SS58_PREFIX: u8 = 42;

pub(crate) type HashFor<B> = <B as BlockT>::Hash;
pub(crate) type NumberFor<B> = <<B as BlockT>::Header as HeaderT>::Number;

#[derive(Clone, Debug)]
pub(crate) enum BlockHashOrNumber<B: BlockT> {
    Hash(HashFor<B>),
    Number(NumberFor<B>),
}

/// Commands of `gear-replay` CLI
#[derive(Debug, Subcommand)]
pub enum Command {
    ReplayBlock(replay_block::ReplayBlockCmd<Block>),
    GearRun(gear_run::GearRunCmd<Block>),
}

/// Parameters shared across the subcommands
#[derive(Clone, Debug, Parser)]
#[group(skip)]
pub struct SharedParams {
    /// The RPC url.
    #[arg(
		short,
		long,
		value_parser = parse::url,
		default_value = "wss://archive-rpc.vara-network.io:443"
	)]
    uri: String,

    /// Sets a custom logging filter. Syntax is `<target>=<level>`, e.g. -lsync=debug.
    ///
    /// Log levels (least to most verbose) are error, warn, info, debug, and trace.
    /// By default, all targets log `info`. The global log level can be set with `-l<level>`.
    #[arg(short = 'l', long, value_name = "NODE_LOG", num_args = 0..)]
    pub log: Vec<String>,
}

#[derive(Debug, Parser)]
struct ReplayCli {
    #[clap(flatten)]
    pub shared: SharedParams,

    /// Commands.
    #[command(subcommand)]
    pub command: Command,
}

impl ReplayCli {
    fn log_filters(&self) -> sc_cli::Result<String> {
        Ok(self.shared.log.join(","))
    }

    fn init_logger(&self) -> sc_cli::Result<()> {
        let logger = LoggerBuilder::new(self.log_filters()?);
        Ok(logger.init()?)
    }
}

pub async fn run() -> sc_cli::Result<()> {
    let options = ReplayCli::parse();

    options.init_logger()?;

    let ss58_prefix = match options.shared.uri.contains("vara") {
        true => VARA_SS58_PREFIX,
        false => GEAR_SS58_PREFIX,
    };
    sp_core::crypto::set_default_ss58_version(ss58_prefix.try_into().unwrap());

    match &options.command {
        Command::ReplayBlock(cmd) => {
            cmd::replay_block::replay_block::<Block>(options.shared.clone(), cmd.clone()).await?
        }
        Command::GearRun(cmd) => {
            cmd::gear_run::gear_run::<Block>(options.shared.clone(), cmd.clone()).await?
        }
    }

    Ok(())
}
