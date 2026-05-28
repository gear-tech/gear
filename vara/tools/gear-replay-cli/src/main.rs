// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::Parser;
use gear_replay_cli::cmd::ReplayCli;

#[tokio::main]
async fn main() {
    let cli = ReplayCli::parse();

    cli.init_logger().expect("Failed to initialize logger");

    cli.run().await.unwrap();
}
