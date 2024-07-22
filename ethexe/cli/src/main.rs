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

mod args;
mod chain_spec;
mod config;
mod metrics;
mod params;
mod service;

#[cfg(test)]
mod tests;

use crate::{
    args::{Args, ArgsOnConfig},
    config::Config,
    service::Service,
};
use anyhow::Context;
use clap::Parser;
use env_logger::Env;
use std::{env, fs};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let optional_config_path = env::current_dir()?.join(".ethexe.toml");
    let args = {
        if fs::metadata(&optional_config_path).is_ok() {
            // logging might be uninitialized at this point due to it might depend on args.
            println!(
                "❗️❗️Using configuration path: {}",
                optional_config_path.display()
            );
            let str = fs::read_to_string(optional_config_path)?;
            let mut file_args: Args = toml::from_str(&str)?;
            file_args.extra_command = ArgsOnConfig::parse().extra_command;
            file_args
        } else {
            Args::parse()
        }
    };

    let config =
        Config::try_from(args.clone()).with_context(|| "Failed to create configuration")?;

    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .try_init()
        .with_context(|| "Failed to initialize logger")?;

    print_info(&config);

    if let Some(extra_command) = args.extra_command {
        extra_command.run(&config).await?;
    } else {
        let mut service = Some(Service::new(&config).await?);

        async fn run_service(service: &mut Option<Service>) -> anyhow::Result<()> {
            if let Some(service) = service.take() {
                service.run().await
            } else {
                futures::future::pending().await
            }
        }

        loop {
            tokio::select! {
                res = run_service(&mut service) => {
                    res?;
                }
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Received SIGINT, shutting down");
                    break;
                }
            }
        }
    }

    Ok(())
}

fn print_info(config: &Config) {
    log::info!("💾 Database: {}", config.database_path.display());
    log::info!("🔑 Key directory: {}", config.key_path.display());
    log::info!(
        "🛜 Network directory: {}",
        config.net_config.config_dir.display()
    );
    log::info!("⧫  Ethereum observer RPC: {}", config.ethereum_rpc);
    log::info!(
        "📡 Ethereum router address: {}",
        config.ethereum_router_address
    );
}
