// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

use anyhow::ensure;
use clap::{ArgGroup, Parser};
use frame_remote_externalities::{Mode, OnlineConfig, SnapshotConfig, Transport};
use gclient::{GearApi, WSAddress};
use gear_runtime::Block;
use std::path::PathBuf;
use tokio::process::Command;

/// Utility program to work with node migrations
#[derive(Parser)]
enum Cli {
    TakeSnapshot(TakeSnapshotCmd),
    RuntimeUpgrade(RuntimeUpgradeCmd),
}

/// Take snapshot of the node
#[derive(Parser)]
#[clap(group(
    ArgGroup::new("node")
        .required(true)
        .multiple(false)
        .args(&["uri", "run_node"])
))]
struct TakeSnapshotCmd {
    /// Node address
    #[arg(long)]
    uri: Option<String>,
    /// Run node to take snapshot
    #[arg(long)]
    run_node: Option<PathBuf>,
    /// Where write snapshot to
    #[arg(short, long)]
    output: PathBuf,
}

impl TakeSnapshotCmd {
    async fn run(self) -> anyhow::Result<()> {
        let api;
        let uri = if let Some(path) = self.run_node {
            api = GearApi::dev_from_path(path).await?;
            api.node_ws_address().unwrap().url()
        } else {
            let uri = self.uri.unwrap();
            if uri == "gear-testnet" {
                WSAddress::gear().url()
            } else {
                uri
            }
        };

        let _ext = frame_remote_externalities::Builder::<Block>::new()
            .mode(Mode::Online(OnlineConfig {
                state_snapshot: Some(SnapshotConfig::new(self.output)),
                transport: Transport::Uri(uri),
                ..Default::default()
            }))
            .build()
            .await
            .map_err(|err| anyhow::anyhow!("Failed to build extension: {}", err))?;

        Ok(())
    }
}

/// Do runtime upgrade on the node snapshot
#[derive(Parser)]
struct RuntimeUpgradeCmd {
    /// Node executable
    #[arg(long, default_value = "target/release/gear")]
    node: PathBuf,
    /// Path to `.wasm` runtime
    #[arg(long)]
    runtime: PathBuf,
    /// Path to snapshot file
    #[arg(short, long)]
    snapshot_path: PathBuf,
}

impl RuntimeUpgradeCmd {
    async fn run(self) -> anyhow::Result<()> {
        let status = Command::new(self.node)
            .arg("try-runtime")
            .arg("--dev")
            .arg("--runtime")
            .arg(self.runtime)
            .arg("on-runtime-upgrade")
            .arg("snap")
            .arg("-s")
            .arg(self.snapshot_path)
            .status()
            .await?;
        ensure!(status.success());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Cli::parse();
    match args {
        Cli::TakeSnapshot(cmd) => cmd.run().await?,
        Cli::RuntimeUpgrade(cmd) => cmd.run().await?,
    }

    Ok(())
}
