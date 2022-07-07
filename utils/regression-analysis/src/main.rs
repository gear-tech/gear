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
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};
use tabled::{Style, Table};

mod junit_tree;
mod output;

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
        input_file: PathBuf,
        #[clap(long, value_parser)]
        output_file: PathBuf,
        #[clap(long, value_parser)]
        current_junit_path: PathBuf,
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

fn median(values: &[u64]) -> u64 {
    assert!(!values.is_empty());

    let len = values.len();
    if len % 2 == 0 {
        let i = len / 2;
        values[i - 1] / 2 + values[i] / 2 + values[i - 1] % 2 + values[i] % 2
    } else {
        values[len / 2]
    }
}

fn average(values: &[u64]) -> u64 {
    values.iter().sum::<u64>() / values.len() as u64
}

fn std(values: &[u64]) -> u64 {
    let average = average(values);
    let sum = values
        .iter()
        .map(|x| x.abs_diff(average).pow(2))
        .sum::<u64>();
    let div = sum / values.len() as u64;
    (div as f64).sqrt() as u64
}

fn collect_data<P: AsRef<Path>>(
    data_folder_path: P,
    output_path: P,
    disable_filter: bool,
    preallocate: usize,
) {
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
                    let mut time_vec = Vec::with_capacity(preallocate);
                    time_vec.push(time);

                    previous_times.insert(key.clone(), time_vec);
                }
            }
        }
    }

    let writer = std::fs::File::create(output_path).unwrap();
    serde_json::to_writer_pretty(writer, &statistics).unwrap();
}

fn output_from_stats(
    mut statistics: BTreeMap<String, BTreeMap<String, Vec<u64>>>,
    current_junit_path: PathBuf,
    disable_filter: bool,
) -> BTreeMap<String, Vec<output::Test>> {
    let executions = build_tree(disable_filter, current_junit_path);
    executions
        .iter()
        .filter_map(|(key, tests)| {
            statistics.get_mut(key).map(|test_times| {
                let test_stats = tests
                    .iter()
                    .filter_map(|(key, &time)| {
                        test_times.get_mut(key).map(|times| {
                            // this is necessary as the order may be wrong after deserialization
                            times.sort_unstable();
                            let len = times.len();
                            let len_remainder = len % 2;
                            let quartile_lower = median(&times[..len / 2]);
                            let quartile_upper = median(&times[len / 2 + len_remainder..]);
                            let median = median(times.as_ref());
                            let average = average(times);
                            let std = std(times);

                            output::Test {
                                name: key.clone(),
                                current_time: (1_000_000_000.0 * time) as u64,
                                median,
                                average,
                                std,
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
        .collect::<BTreeMap<_, _>>()
}

fn compare(data_path: PathBuf, current_junit_path: PathBuf, disable_filter: bool) {
    let statistics: BTreeMap<String, BTreeMap<String, Vec<u64>>> =
        serde_json::from_str(&fs::read_to_string(data_path).unwrap()).unwrap();
    let mut compared = output_from_stats(statistics, current_junit_path, disable_filter);

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

fn convert(input_file: PathBuf, output_file: PathBuf, current_junit_path: PathBuf) {
    #[derive(Debug, Serialize)]
    struct GithubActionBenchmark {
        name: String,
        unit: String,
        value: u64,
        range: Option<String>,
        extra: Option<String>,
    }

    let input_file = fs::read_to_string(input_file).unwrap();
    let stats = serde_json::from_str(&input_file).unwrap();
    let outputs = output_from_stats(stats, current_junit_path, false);

    let mut benchmarks = vec![];
    for (section_name, tests) in outputs {
        for test in tests {
            let benchmark = GithubActionBenchmark {
                name: test.name,
                unit: "ns".to_string(),
                value: test.current_time,
                range: Some(format!("± {}", test.std)),
                extra: Some(section_name.clone()),
            };
            benchmarks.push(benchmark);
        }
    }

    let output = serde_json::to_string(&benchmarks).unwrap();
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
            collect_data(data_folder_path, output_path, disable_filter, PREALLOCATE);
        }
        Commands::Compare {
            data_path,
            current_junit_path,
            disable_filter,
        } => {
            compare(data_path, current_junit_path, disable_filter);
        }
        Commands::Convert {
            input_file,
            output_file,
            current_junit_path,
        } => {
            convert(input_file, output_file, current_junit_path);
        }
    }
}
