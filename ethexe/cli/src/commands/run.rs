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

use crate::{
    Params,
    params::{MergeParams, NetworkParams, NodeParams},
};
use anyhow::{Context as _, Result};
use clap::Args;
use ethexe_service::{Service, config::Config};
use std::time::Duration;
use tokio::{runtime::Builder, task::JoinSet};

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
    /// Default block time (dev mode) in seconds.
    const DEFAULT_DEV_BLOCK_TIME: u64 = 1;

    /// Merge the command with the provided params.
    pub fn with_params(mut self, params: Params) -> Self {
        self.params = self.params.merge(params);

        self
    }

    /// Run the ethexe service (node).
    pub fn run(mut self) -> Result<()> {
        let default = if self.verbose { "debug" } else { "info" };
        crate::enable_logging(default)?;

        let mut anvil_instance = None;
        let mut extra_validator_keys = Vec::new();

        if let Some(node) = self.params.node.as_mut()
            && node.dev
        {
            // set block time to 1 second if not set explicitly
            let block_time = Duration::from_secs(
                self.params
                    .ethereum
                    .as_ref()
                    .and_then(|ethereum| ethereum.block_time)
                    .unwrap_or(Self::DEFAULT_DEV_BLOCK_TIME),
            );
            let pre_funded_accounts = node
                .pre_funded_accounts
                .unwrap_or(NodeParams::DEFAULT_PRE_FUNDED_ACCOUNTS)
                .get();
            let dev_validators = node
                .dev_validators
                .unwrap_or(NodeParams::DEFAULT_DEV_VALIDATORS)
                .get();
            let (anvil, validator_public_keys, router_address) = Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(Service::configure_dev_environment(
                    node.keys_dir(),
                    block_time,
                    pre_funded_accounts,
                    dev_validators,
                ))?;

            let primary_key = validator_public_keys[0];
            node.validator = Some(primary_key.to_string());
            node.validator_session = Some(primary_key.to_string());
            if node.canonical_quarantine.is_none() {
                // disable quarantine in dev mode if not set explicitly
                node.canonical_quarantine = Some(0);
            }

            let ethereum = self.params.ethereum.get_or_insert_with(Default::default);
            ethereum.ethereum_rpc = Some(anvil.ws_endpoint());
            ethereum.ethereum_beacon_rpc = Some(anvil.endpoint());
            ethereum.ethereum_router = Some(router_address);
            ethereum.block_time = Some(block_time.as_secs());

            // make sure RPC is enabled as RPC is disabled by default
            self.params.rpc.get_or_insert_with(Default::default);

            if dev_validators > 1 {
                // enable network for multi-validator mode
                self.params
                    .network
                    .get_or_insert_with(NetworkParams::dev_defaults);

                extra_validator_keys = validator_public_keys[1..].to_vec();
            }

            anvil_instance = Some(anvil);
        }

        let config = self
            .params
            .into_config()
            .with_context(|| "invalid configuration")?;

        config.log_info();

        let mut builder = Builder::new_multi_thread();

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
                if extra_validator_keys.is_empty() {
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
                } else {
                    Self::run_multi_validator(config, extra_validator_keys).await
                }
            })
    }

    async fn run_multi_validator(
        primary_config: Config,
        extra_validator_keys: Vec<gsigner::secp256k1::PublicKey>,
    ) -> Result<()> {
        let mut tasks = JoinSet::new();

        // Spawn extra validator services, each pinned to its own dedicated thread
        // The reason for this is that we have a few `thread_local!` storages that
        // are not designed to be shared across multiple nodes in the *same* process.
        for (i, key) in extra_validator_keys.iter().enumerate() {
            let validator_config = primary_config.clone_for_dev_validator(key, i + 1)?;
            let idx = i + 1;

            tasks.spawn_blocking(move || {
                log::info!("🚀 Starting validator-{idx} on dedicated thread");

                Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create tokio runtime for validator")
                    .block_on(async {
                        let service = Service::new(&validator_config)
                            .await
                            .with_context(|| format!("failed to create validator-{idx} service"))?;
                        service.run().await
                    })
            });
        }

        // Run primary validator on its own dedicated thread as well
        tasks.spawn_blocking(move || {
            log::info!("🚀 Starting validator-0 (primary) on dedicated thread");

            Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for primary validator")
                .block_on(async {
                    let service = Service::new(&primary_config)
                        .await
                        .with_context(|| "failed to create primary validator service")?;
                    service.run().await
                })
        });

        tokio::select! {
            Some(result) = tasks.join_next() => {
                result.with_context(|| "validator task panicked")?
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down");
                Ok(())
            }
        }
    }
}
