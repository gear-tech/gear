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
use frame_support::dispatch::GetCallName;
use junit_common::TestSuites;
use pallet_gear::{HostFnWeights, InstructionWeights};
use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs, iter,
    path::{Path, PathBuf},
    str::FromStr,
};
use tabled::{Style, Table};

mod junit_tree;
mod output;
mod stats;

const PALLET_NAMES: [&str; 6] = [
    "pallet-gear-gas",
    "pallet-gear",
    "pallet-gear-debug",
    "pallet-gear-messenger",
    "pallet-gear-payment",
    "pallet-gear-scheduler",
];

const PREALLOCATE: usize = 1_000;

const TEST_SUITES_TEXT: &str = "Test suites";

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CollectData {
        #[arg(long, value_parser)]
        data_folder_path: PathBuf,
        #[arg(long, value_parser)]
        output_path: PathBuf,
        #[arg(long, value_parser)]
        disable_filter: bool,
    },
    Compare {
        #[arg(long, value_parser)]
        data_path: PathBuf,
        #[arg(long, value_parser)]
        current_junit_path: PathBuf,
        #[arg(long, value_parser)]
        disable_filter: bool,
    },
    Convert {
        #[arg(long, value_parser)]
        data_folder_path: PathBuf,
        #[arg(long, value_parser)]
        output_file: PathBuf,
        #[arg(long, value_parser)]
        disable_filter: bool,
    },
    Weights {
        #[command(subcommand)]
        kind: WeightsKind,
        #[arg(long, value_parser)]
        input_file: PathBuf,
        #[arg(long, value_parser)]
        output_file: PathBuf,
    },
}

#[derive(Subcommand)]
enum WeightsKind {
    HostFn,
    Instruction,
    Extrinsic,
}

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

#[derive(Deserialize)]
#[serde(transparent)]
struct WeightBenchmark(Vec<u64>);

impl WeightBenchmark {
    fn calc_weight(&self) -> u64 {
        self.0.iter().sum()
    }
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
        let executions = build_tree(disable_filter, entry.unwrap().path());
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
        let mut table = Table::new(total_time);
        table.with(Style::markdown());
        println!("{table}");
        println!();
    }

    for (name, stats) in compared {
        println!("name = {name}");
        let mut table = Table::new(stats);
        table.with(Style::markdown());
        println!("{table}");
        println!();
    }
}

fn convert(data_folder_path: PathBuf, output_file: PathBuf, disable_filter: bool) {
    let statistics = collect_data(data_folder_path, disable_filter);
    let benchmarks = statistics
        .into_iter()
        .flat_map(|(section_name, test_times)| iter::repeat(section_name).zip(test_times))
        .map(|(section_name, (test_name, mut times))| {
            let test_name = if section_name == TEST_SUITES_TEXT {
                test_name
            } else {
                format!("{section_name} - {test_name}")
            };

            output::Test::new_for_github(test_name, &mut times)
        })
        .map(|test| GithubActionBenchmark {
            name: test.name,
            unit: "ms".to_string(),
            value: test.current_time / 1_000_000,
            range: Some(format!("± {}", test.std_dev / 1_000_000)),
            extra: None,
        })
        .collect::<Vec<_>>();

    let output = serde_json::to_string_pretty(&benchmarks).unwrap();
    fs::write(output_file, output).unwrap();
}

fn weights(kind: WeightsKind, input_file: PathBuf, output_file: PathBuf) {
    fn convert_into_bench(
        map: &HashMap<String, WeightBenchmark>,
        field: &str,
    ) -> Option<GithubActionBenchmark> {
        map.get(field).map(|weight| GithubActionBenchmark {
            name: field.to_string(),
            unit: "ns".to_string(),
            value: weight.calc_weight() / 1000,
            range: None,
            extra: None,
        })
    }

    macro_rules! add_weights {
        (
            weights = $weights:ident;
            benches = $benches:ident;
            $name:ident {
                $( $field:ident $( : $underscore:tt )?, )+
            }
        ) => {{
            // check field is exist
            let $name::<gear_runtime::Runtime> {
                $( $field: _, )+
            } = Default::default();

            $(
                let field = add_weights!(@field $weights $field $( : $underscore )?);
                $benches.extend(field);
            )+
        }};
        (@field $weights:ident $field:ident: _) => { None };
        (@field $weights:ident _phantom) => { None };
        (@field $weights:ident $field:ident) => {
            convert_into_bench(&$weights, stringify!($field))
        };
    }

    let file = fs::File::open(input_file).unwrap();
    let map: HashMap<String, WeightBenchmark> = serde_json::from_reader(file).unwrap();
    let map: HashMap<String, WeightBenchmark> = map
        .into_iter()
        .map(|(name, bench)| {
            (
                // we strip prefix because WASM instruction benchmarks have it
                name.strip_prefix("instr_")
                    .map(|x| x.to_string())
                    .unwrap_or(name),
                bench,
            )
        })
        .collect();

    let mut benches = vec![];

    match kind {
        WeightsKind::HostFn => {
            add_weights! {
                weights = map;
                benches = benches;
                HostFnWeights {
                    _phantom,
                    alloc,
                    free,
                    gr_gas_available,
                    gr_message_id,
                    gr_origin,
                    gr_program_id,
                    gr_source,
                    gr_value,
                    gr_value_available,
                    gr_size,
                    gr_read,
                    gr_read_per_byte,
                    gr_block_height,
                    gr_block_timestamp,
                    gr_random,
                    gr_send_init,
                    gr_send_push,
                    gr_send_push_per_byte,
                    gr_send_commit,
                    gr_send_commit_per_byte,
                    gr_reservation_send_commit,
                    gr_reservation_send_commit_per_byte,
                    gr_reply_commit,
                    gr_reply_commit_per_byte,
                    gr_reservation_reply_commit,
                    gr_reservation_reply_commit_per_byte,
                    gr_reply_push,
                    gr_reply_push_per_byte,
                    gr_reply_to,
                    gr_signal_from,
                    gr_reply_push_input,
                    gr_reply_push_input_per_byte,
                    gr_send_push_input,
                    gr_send_push_input_per_byte,
                    gr_debug,
                    gr_debug_per_byte,
                    gr_error,
                    gr_status_code,
                    gr_exit,
                    gr_leave,
                    gr_wait,
                    gr_wait_for,
                    gr_wait_up_to,
                    gr_wake,
                    gr_create_program_wgas,
                    gr_create_program_wgas_payload_per_byte,
                    gr_create_program_wgas_salt_per_byte,
                    gr_reserve_gas,
                    gr_unreserve_gas,
                    gr_system_reserve_gas,
                }
            }
        }
        WeightsKind::Instruction => {
            add_weights! {
                weights = map;
                benches = benches;
                InstructionWeights {
                    version: _,
                    i64const,
                    i64load,
                    i32load,
                    i64store,
                    i32store,
                    select,
                    r#if,
                    br,
                    br_if,
                    br_table,
                    br_table_per_entry,
                    call,
                    call_indirect,
                    call_indirect_per_param,
                    call_per_local,
                    local_get,
                    local_set,
                    local_tee,
                    global_get,
                    global_set,
                    memory_current,
                    i64clz,
                    i32clz,
                    i64ctz,
                    i32ctz,
                    i64popcnt,
                    i32popcnt,
                    i64eqz,
                    i32eqz,
                    i64extendsi32,
                    i64extendui32,
                    i32wrapi64,
                    i64eq,
                    i32eq,
                    i64ne,
                    i32ne,
                    i64lts,
                    i32lts,
                    i64ltu,
                    i32ltu,
                    i64gts,
                    i32gts,
                    i64gtu,
                    i32gtu,
                    i64les,
                    i32les,
                    i64leu,
                    i32leu,
                    i64ges,
                    i32ges,
                    i64geu,
                    i32geu,
                    i64add,
                    i32add,
                    i64sub,
                    i32sub,
                    i64mul,
                    i32mul,
                    i64divs,
                    i32divs,
                    i64divu,
                    i32divu,
                    i64rems,
                    i32rems,
                    i64remu,
                    i32remu,
                    i64and,
                    i32and,
                    i64or,
                    i32or,
                    i64xor,
                    i32xor,
                    i64shl,
                    i32shl,
                    i64shrs,
                    i32shrs,
                    i64shru,
                    i32shru,
                    i64rotl,
                    i32rotl,
                    i64rotr,
                    i32rotr,
                    _phantom,
                }
            }
        }
        WeightsKind::Extrinsic => {
            let extrinsics = pallet_gear::pallet::Call::<gear_runtime::Runtime>::get_call_names();
            benches.extend(
                extrinsics
                    .iter()
                    .flat_map(|extrinsic| convert_into_bench(&map, extrinsic)),
            );
        }
    }

    let output_file = fs::File::create(output_file).unwrap();
    serde_json::to_writer_pretty(output_file, &benches).unwrap();
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
        Commands::Weights {
            kind,
            input_file,
            output_file,
        } => weights(kind, input_file, output_file),
    }
}
