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

use clap::{Parser, Subcommand};
use common::TestSuites;
use quick_xml::de::from_str;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs, iter,
    path::{Path, PathBuf},
    str::FromStr,
};
use tabled::{Style, Table};

mod junit_tree;
mod output;
mod stats;

const PALLET_NAMES: [&str; 7] = [
    "pallet-gear-gas",
    "pallet-gear",
    "pallet-gear-debug",
    "pallet-gear-messenger",
    "pallet-gear-program",
    "pallet-gear-payment",
    "pallet-usage",
];

const PREALLOCATE: usize = 1_000;

const TEST_SUITES_TEXT: &str = "Test suites";

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CollectData {
        #[clap(long, value_parser)]
        data_folder_path: PathBuf,
        #[clap(long, value_parser)]
        output_path: PathBuf,
        #[clap(long, value_parser)]
        disable_filter: bool,
    },
    Compare {
        #[clap(long, value_parser)]
        data_path: PathBuf,
        #[clap(long, value_parser)]
        current_junit_path: PathBuf,
        #[clap(long, value_parser)]
        disable_filter: bool,
    },
    Convert {
        #[clap(long, value_parser)]
        data_folder_path: PathBuf,
        #[clap(long, value_parser)]
        output_file: PathBuf,
        #[clap(long, value_parser)]
        disable_filter: bool,
    },
}

fn build_tree<P: AsRef<Path>>(
    disable_filter: bool,
    path: P,
) -> BTreeMap<String, BTreeMap<String, f64>> {
    let filter = |pallet_name: &str| {
        if disable_filter {
            return true;
        }

        PALLET_NAMES.iter().any(|&name| name == pallet_name)
    };

    let junit_xml = fs::read_to_string(path).unwrap();
    let test_suites: TestSuites = from_str(&junit_xml).unwrap();
    let total_time = [(
        String::from("Total time"),
        f64::from_str(&test_suites.time).unwrap(),
    )]
    .into();
    let mut result = junit_tree::build_tree(filter, test_suites);
    result.insert(String::from(TEST_SUITES_TEXT), total_time);
    result
}

fn collect_data(
    data_folder_path: PathBuf,
    disable_filter: bool,
) -> BTreeMap<String, BTreeMap<String, Vec<u64>>> {
    let mut statistics: BTreeMap<_, BTreeMap<_, Vec<_>>> = BTreeMap::default();
    for entry in fs::read_dir(data_folder_path).unwrap() {
        let executions = build_tree(disable_filter, &entry.unwrap().path());
        for (ref key, ref times) in executions {
            if !statistics.contains_key(key) {
                statistics.insert(key.clone(), Default::default());
            }

            let previous_times = statistics.get_mut(key).unwrap();
            for (key, &time) in times {
                let time = (1_000_000_000.0 * time) as u64;

                if let Some(time_vec) = previous_times.get_mut(key) {
                    let i = match time_vec.binary_search(&time) {
                        Ok(i) => i,
                        Err(i) => i,
                    };

                    time_vec.insert(i, time);
                } else {
                    let mut time_vec = Vec::with_capacity(PREALLOCATE);
                    time_vec.push(time);

                    previous_times.insert(key.clone(), time_vec);
                }
            }
        }
    }

    statistics
}

fn compare(data_path: PathBuf, current_junit_path: PathBuf, disable_filter: bool) {
    let mut statistics: BTreeMap<String, BTreeMap<String, Vec<u64>>> =
        serde_json::from_str(&fs::read_to_string(data_path).unwrap()).unwrap();
    let executions = build_tree(disable_filter, current_junit_path);
    let mut compared = executions
        .iter()
        .filter_map(|(key, tests)| {
            statistics.get_mut(key).map(|test_times| {
                let test_stats = tests
                    .iter()
                    .filter_map(|(key, &time)| {
                        test_times
                            .get_mut(key)
                            .map(|times| output::Test::new_for_stats(key.clone(), time, times))
                    })
                    .collect::<Vec<_>>();

                (key.clone(), test_stats)
            })
        })
        .collect::<BTreeMap<_, _>>();

    if let Some(total_time) = compared.remove(TEST_SUITES_TEXT) {
        println!("Total execution time");
        let table = Table::new(total_time).with(Style::github_markdown().header_intersection('|'));
        println!("{}", table);
        println!();
    }

    for (name, stats) in compared {
        println!("name = {}", name);
        let table = Table::new(stats).with(Style::github_markdown().header_intersection('|'));
        println!("{}", table);
        println!();
    }
}

fn convert(data_folder_path: PathBuf, output_file: PathBuf, disable_filter: bool) {
    #[derive(Debug, Serialize)]
    struct GithubActionBenchmark {
        name: String,
        unit: String,
        value: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extra: Option<String>,
    }

    let statistics = collect_data(data_folder_path.clone(), disable_filter);
    let benchmarks = statistics
        .into_iter()
        .flat_map(|(section_name, test_times)| iter::repeat(section_name).zip(test_times))
        .map(|(section_name, (test_name, mut times))| {
            let test_name = if section_name == TEST_SUITES_TEXT {
                test_name
            } else {
                format!("{} - {}", section_name, test_name)
            };

            output::Test::new_for_github(test_name, &mut times)
        })
        .map(|test| GithubActionBenchmark {
            name: test.name,
            unit: "ns".to_string(),
            value: test.current_time,
            range: Some(format!("± {}", test.std_dev)),
            extra: None,
        })
        .collect::<Vec<_>>();

    let output = serde_json::to_string_pretty(&benchmarks).unwrap();
    fs::write(output_file, output).unwrap();
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::CollectData {
            data_folder_path,
            disable_filter,
            output_path,
        } => {
            let statistics = collect_data(data_folder_path, disable_filter);
            let writer = fs::File::create(output_path).unwrap();
            serde_json::to_writer_pretty(writer, &statistics).unwrap();
        }
        Commands::Compare {
            data_path,
            current_junit_path,
            disable_filter,
        } => {
            compare(data_path, current_junit_path, disable_filter);
        }
        Commands::Convert {
            data_folder_path,
            output_file,
            disable_filter,
        } => {
            convert(data_folder_path, output_file, disable_filter);
        }
    }
}
