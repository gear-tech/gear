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
mod config;
mod params;
mod service;

use crate::{args::Args, config::Config, service::Service};
use anyhow::Context;
use clap::Parser;
use env_logger::Env;
use std::{env, fs};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let optional_config_path = env::current_dir()?.join(".hypercore.toml");
    let args = {
        let cli_args = Args::parse();
        if fs::metadata(&optional_config_path).is_ok() {
            // logging might be uninitialized at this point due to it might depend on args.
            println!(
                "❗️❗️Using configuration path: {}",
                optional_config_path.display()
            );
            let str = fs::read_to_string(optional_config_path)?;
            let mut file_args: Args = toml::from_str(&str)?;
            file_args.extra_command = cli_args.extra_command;
            file_args
        } else {
            cli_args
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
        let service = Service::new(&config).await?;

        service.run().await?;
    }

    Ok(())
}

fn print_info(config: &Config) {
    log::info!("💾 Database: {}", config.database_path.display());
    log::info!("🔑 Key directory: {}", config.key_path.display());
    log::info!("🛜 Network directory: {}", config.network_path.display());
    log::info!("⧫  Ethereum observer RPC: {}", config.ethereum_rpc);
    log::info!(
        "📡 Ethereum router address: {}",
        config.ethereum_router_address
    );
}
