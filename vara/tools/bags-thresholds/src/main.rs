// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Make the set of bag thresholds to be used with pallet-bags-list.

use clap::Parser;
use generate_bags::generate_thresholds;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Opt {
    /// How many bags to generate.
    #[arg(long, default_value_t = 200)]
    n_bags: usize,

    /// Where to write the output.
    output: PathBuf,

    /// The total issuance of the currency used to create `VoteWeight`.
    #[arg(short, long)]
    total_issuance: u128,

    /// The minimum account balance (i.e. existential deposit) for the currency used to create
    /// `VoteWeight`.
    #[arg(short, long)]
    minimum_balance: u128,
}

fn main() -> Result<(), std::io::Error> {
    let Opt {
        n_bags,
        output,
        total_issuance,
        minimum_balance,
    } = Opt::parse();
    generate_thresholds::<vara_runtime::Runtime>(n_bags, &output, total_issuance, minimum_balance)
}
