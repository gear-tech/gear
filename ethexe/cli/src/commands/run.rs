// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{Params, params::MergeParams};
use anyhow::{Context as _, Result, anyhow};
use clap::Args;
use ethexe_service::Service;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

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
    pub fn run(self) -> Result<()> {
        let default = if self.verbose { "debug" } else { "info" };

        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(default.parse()?)
                    .from_env_lossy()
                    .add_directive("wasmtime_cranlift=off".parse()?)
                    .add_directive("cranelift=off".parse()?),
            )
            .try_init()
            .map_err(|e| anyhow!("failed to initialize logger: {e}"))?;

        let config = self
            .params
            .into_config()
            .with_context(|| "invalid configuration")?;

        config.log_info();

        let mut builder = tokio::runtime::Builder::new_multi_thread();

        if let Some(worker_threads) = config.node.worker_threads {
            builder.worker_threads(worker_threads);
        }

        if let Some(blocking_threads) = config.node.blocking_threads {
            builder.max_blocking_threads(blocking_threads);
        }

        builder
            // 30 seconds should be enough to keep blocking threads alive between block processing
            .thread_keep_alive(Duration::from_secs(30))
            .enable_all()
            .build()
            .expect("failed to create tokio runtime")
            .block_on(async {
                let service = Service::new(&config)
                    .await
                    .with_context(|| "failed to create ethexe primary service")?;

                tokio::select! {
                    res = service.run() => res,
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("Received SIGINT, shutting down");
                        Ok(())
                    }
                }
            })
    }
}
