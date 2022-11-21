// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Make the set of bag thresholds to be used with pallet-bags-list.

use clap::{ArgEnum, Parser};
use generate_bags::generate_thresholds;
use std::path::{Path, PathBuf};
use vara_runtime::Runtime as VaraRuntime;

#[derive(Clone, Debug, ArgEnum)]
#[clap(rename_all = "PascalCase")]
enum Runtime {
    // TODO: uncomment once gear runtime implements pallet_staking::Config
    // Gear,
    Vara,
}

impl Runtime {
    #[allow(clippy::type_complexity)]
    fn generate_thresholds_fn(
        &self,
    ) -> Box<dyn FnOnce(usize, &Path, u128, u128) -> Result<(), std::io::Error>> {
        match self {
            Runtime::Vara => Box::new(generate_thresholds::<VaraRuntime>),
        }
    }
}

#[derive(Debug, Parser)]
struct Opt {
    /// How many bags to generate.
    #[clap(long, default_value = "200")]
    n_bags: usize,

    /// Which runtime to generate.
    #[clap(long, ignore_case = true, arg_enum, default_value = "Vara")]
    runtime: Runtime,

    /// Where to write the output.
    output: PathBuf,

    /// The total issuance of the native currency.
    #[clap(short, long)]
    total_issuance: u128,

    /// The minimum account balance (i.e. existential deposit) for the native currency.
    #[clap(short, long)]
    minimum_balance: u128,
}

fn main() -> Result<(), std::io::Error> {
    let Opt {
        n_bags,
        output,
        runtime,
        total_issuance,
        minimum_balance,
    } = Opt::parse();

    runtime.generate_thresholds_fn()(n_bags, &output, total_issuance, minimum_balance)
}
