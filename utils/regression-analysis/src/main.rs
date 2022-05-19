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

use std::collections::BTreeMap;

use std::path::{Path, PathBuf};
use std::fs;

use clap::{Parser, Subcommand};

use quick_xml::de::from_str;

mod junit_parser;
mod output;

use common::TestSuites;

const PALLET_NAMES: [&str; 7] = [
    "pallet-gas",
    "pallet-gear",
    "pallet-gear-debug",
    "pallet-gear-messenger",
    "pallet-gear-program",
    "pallet-gear-payment",
    "pallet-usage",
];

const PREALLOCATE: usize = 1_000;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    CollectData {
        #[clap(long)]
        data_folder_path: PathBuf,
        #[clap(long)]
        output_path: PathBuf,
        #[clap(long)]
        disable_filter: bool,
    },
    Compare,
}

fn build_tree(disable_filter: bool, path: &Path) -> BTreeMap<String, BTreeMap<String, f64>> {
    let filter = |pallet_name: &str| {
        if disable_filter {
            return true;
        }

        PALLET_NAMES.iter().any(|&name| name == pallet_name)
    };

    let junit_xml = std::fs::read_to_string(path).unwrap();
    let test_suites: TestSuites = from_str(&junit_xml).unwrap();
    junit_parser::build_tree(filter, test_suites)
}

fn median(values: &[u64]) -> u64 {
    assert!(!values.is_empty());

    let len = values.len();
    if len % 2 == 0 {
        let i = len / 2;
        values[i - 1] / 2 + values[i + 1] / 2 + values[i - 1] % 2 + values[i + 1] % 2
    } else {
        values[len / 2 + 1]
    }
}

fn collect_data(data_folder_path: &Path, output_path: &Path, disable_filter: bool, preallocate: usize) {
    let mut statistics: BTreeMap<String, BTreeMap<String, Vec<u64>>> = BTreeMap::default();
    for entry in fs::read_dir(data_folder_path).unwrap() {
        let executions = build_tree(disable_filter, &entry.unwrap().path());
        executions.iter().for_each(|(key, times)| {
            if !statistics.contains_key(key) {
                statistics.insert(key.clone(), Default::default());
            }

            let previous_times = statistics.get_mut(key).unwrap();
            times.iter().for_each(|(key, &time)| {
                let time = (1_000_000_000.0 * time) as u64;

                if let Some(time_vec) = previous_times.get_mut(key) {
                    let i = match time_vec.binary_search(&time) {
                        Ok(i) => i,
                        Err(i) => i,
                    };

                    time_vec.insert(i, time);
                } else {
                    let mut time_vec = Vec::with_capacity(preallocate);
                    time_vec.push(time);

                    previous_times.insert(key.clone(), time_vec);
                }
            });
        });
    }

    let writer = std::fs::File::create(output_path).unwrap();
    serde_json::to_writer_pretty(writer, &statistics).unwrap();
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::CollectData { data_folder_path, disable_filter, output_path } => {
            collect_data(&data_folder_path, &output_path, *disable_filter, PREALLOCATE);
        }
        Commands::Compare => {
        }
    }
}
