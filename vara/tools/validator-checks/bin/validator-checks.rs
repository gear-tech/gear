// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::Parser;
use gear_validator_checks::Opt;

#[tokio::main]
async fn main() {
    Opt::parse().run().await.unwrap()
}
