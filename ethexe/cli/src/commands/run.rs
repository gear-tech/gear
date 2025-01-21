// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use super::MergeParams;
use crate::Params;
use anyhow::{Context as _, Result};
use clap::Args;
use env_logger::Env;
use ethexe_service::Service;
use log::LevelFilter;

/// Run the node.
#[derive(Debug, Args)]
pub struct RunCommand {
    /// CLI parameters to be merged with file ones before execution.
    #[clap(flatten)]
    pub params: Params,

    /// Verbose mode: if enabled turns on debug logs in addition to info level.
    #[clap(short, long)]
    pub verbose: bool,
}

impl RunCommand {
    /// Merge the command with the provided params.
    pub fn with_params(mut self, params: Params) -> Self {
        self.params = self.params.merge(params);

        self
    }

    /// Run the ethexe service (node).
    pub async fn run(self) -> Result<()> {
        let default = if self.verbose { "debug" } else { "info" };

        let env = Env::default().default_filter_or(default);

        env_logger::Builder::from_env(env)
            .filter_module("wasmtime_cranelift", LevelFilter::Off)
            .filter_module("cranelift", LevelFilter::Off)
            .try_init()
            .context("failed to initialize logger")?;

        let config = self.params.into_config().context("invalid configuration")?;

        config.log_info();

        let service = Service::new(&config)
            .await
            .context("failed to create ethexe primary service")?;

        tokio::select! {
            res = service.run() => res,
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down");
                Ok(())
            }
        }
    }
}
