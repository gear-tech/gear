// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::{Parser, Subcommand};
use runtime_primitives::Block;
use sc_tracing::logging::LoggerBuilder;
use std::fmt::Debug;

use crate::shared_parameters::SharedParams;

pub mod create_snapshot;
pub mod gear_run;
pub mod replay_block;

#[derive(Debug, Parser)]
pub struct ReplayCli {
    #[clap(flatten)]
    pub shared: SharedParams,

    /// Commands.
    #[command(subcommand)]
    pub command: Command,
}

impl ReplayCli {
    #[allow(clippy::result_large_err)]
    fn log_filters(&self) -> sc_cli::Result<String> {
        Ok(self.shared.log.join(","))
    }

    #[allow(clippy::result_large_err)]
    pub fn init_logger(&self) -> sc_cli::Result<()> {
        let logger = LoggerBuilder::new(self.log_filters()?);
        Ok(logger.init()?)
    }

    #[allow(clippy::result_large_err)]
    pub async fn run(&self) -> sc_cli::Result<()> {
        self.command.run(&self.shared).await
    }
}

/// Commands of `gear-replay` CLI
#[derive(Debug, Subcommand)]
pub enum Command {
    ReplayBlock(replay_block::ReplayBlockCmd<Block>),
    GearRun(gear_run::GearRunCmd<Block>),
    /// Create a new snapshot file.
    CreateSnapshot(create_snapshot::CreateSnapshotCmd<Block>),
}

impl Command {
    pub async fn run(&self, shared: &SharedParams) -> sc_cli::Result<()> {
        gear_runtime_interface::sandbox_init(
            gear_runtime_interface::SandboxBackend::Wasmtime,
            None,
        );

        match &self {
            Command::ReplayBlock(cmd) => {
                replay_block::run::<Block>(shared.clone(), cmd.clone()).await
            }
            Command::GearRun(cmd) => gear_run::run::<Block>(shared.clone(), cmd.clone()).await,
            Command::CreateSnapshot(cmd) => {
                create_snapshot::run::<Block>(shared.clone(), cmd.clone()).await
            }
        }
    }
}
