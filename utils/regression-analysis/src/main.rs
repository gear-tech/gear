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
    Compare {
        #[clap(long)]
        data_path: PathBuf,
        #[clap(long)]
        current_junit_path: PathBuf,
        #[clap(long)]
        disable_filter: bool,
    },
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

fn compare<P: AsRef<Path>>(data_path: P, current_junit_path: P, disable_filter: bool) {
    let statistics: BTreeMap<String, BTreeMap<String, Vec<u64>>> = serde_json::from_str(&fs::read_to_string(data_path).unwrap()).unwrap();
    let executions = build_tree(disable_filter, current_junit_path.as_ref());
    let compared = executions.iter().filter_map(|(key, tests)| {
        statistics.get(key).map(|test_times| {
            let test_stats = tests
                .iter()
                .filter_map(|(key, &time)| {
                    test_times.get(key).map(|times| {
                        let len = times.len();
                        let len_remainder = len % 2;
                        let quartile_lower = median(&times[..len / 2]);
                        let quartile_upper = median(&times[len / 2 + len_remainder..]);
                        let median = median(times.as_ref());

                        output::Test {
                            name: key.clone(),
                            current_time: (1_000_000_000.0 * time) as u64,
                            median,
                            quartile_lower,
                            quartile_upper,
                            min: *times.first().unwrap(),
                            max: *times.last().unwrap(),
                        }
                    })
                })
                .collect::<Vec<_>>();

            (key.clone(), test_stats)
        })
    })
    .collect::<BTreeMap<_, _>>();

    for (name, stats) in compared {
        println!("name = {}", name);
        let table = tabled::Table::new(stats);
        println!("{}", table);
        println!();
    }
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::CollectData { data_folder_path, disable_filter, output_path } => {
            collect_data(&data_folder_path, &output_path, *disable_filter, PREALLOCATE);
        },
        Commands::Compare { data_path, current_junit_path, disable_filter } => {
            compare(data_path, current_junit_path, *disable_filter);
        }
    }
}
