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

mod weights;

use clap::{Parser, Subcommand};
use frame_support::{traits::Get, weights::Weight};
use junit_common::TestSuites;
use pallet_gear::{HostFnWeights, InstructionWeights, MemoryWeights};
use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs, iter,
    ops::Deref,
    path::{Path, PathBuf},
    str::FromStr,
};
use tabled::{Style, Table};

mod junit_tree;
mod output;
mod runtime;
mod stats;

const PALLET_NAMES: [&str; 7] = [
    "pallet-gear-gas",
    "pallet-gear",
    "pallet-gear-debug",
    "pallet-gear-messenger",
    "pallet-gear-program",
    "pallet-gear-payment",
    "pallet-gear-scheduler",
];

const PREALLOCATE: usize = 1_000;

const TEST_SUITES_TEXT: &str = "Test suites";

static WEIGHTS_JSON: WeightJson = WeightJson::new();

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
    Weights {
        #[clap(subcommand)]
        kind: WeightsKind,
        #[clap(long, value_parser)]
        input_file: PathBuf,
        #[clap(long, value_parser)]
        output_file: PathBuf,
    },
}

#[derive(Subcommand)]
enum WeightsKind {
    HostFn,
    Instruction,
    Memory,
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

struct WeightJson(once_cell::sync::OnceCell<HashMap<String, WeightBenchmark>>);

impl WeightJson {
    const fn new() -> Self {
        Self(once_cell::sync::OnceCell::new())
    }

    fn init(&self, input_file: PathBuf) {
        let file = fs::File::open(input_file).unwrap();
        let map = serde_json::from_reader(file).unwrap();
        self.0.get_or_init(move || map);
    }
}

impl Deref for WeightJson {
    type Target = HashMap<String, WeightBenchmark>;

    fn deref(&self) -> &Self::Target {
        self.0.get().unwrap()
    }
}

#[derive(Deserialize)]
struct WeightBenchmark {
    base_weight: Weight,
    base_reads: Weight,
    base_writes: Weight,
    component_weight: Vec<WeightBenchmarkComponent>,
    component_reads: Vec<WeightBenchmarkComponent>,
    component_writes: Vec<WeightBenchmarkComponent>,
}

impl WeightBenchmark {
    fn calc_weight<T: frame_system::Config>(&self, components: HashMap<&str, Weight>) -> Weight {
        let mut weight = self.base_weight;

        for cw in &self.component_weight {
            weight = weight
                .saturating_add(cw.slope)
                .saturating_mul(cw.name.as_weight(&components));
        }

        if self.base_reads != 0 {
            weight = weight.saturating_add(T::DbWeight::get().reads(self.base_reads));
        }

        for cr in &self.component_reads {
            weight = weight.saturating_add(
                T::DbWeight::get().reads(cr.slope.saturating_mul(cr.name.as_weight(&components))),
            );
        }

        if self.base_writes != 0 {
            weight = weight.saturating_add(T::DbWeight::get().writes(self.base_writes));
        }

        for cw in &self.component_writes {
            weight = weight.saturating_add(
                T::DbWeight::get().writes(cw.slope.saturating_mul(cw.name.as_weight(&components))),
            );
        }

        weight
    }
}

#[derive(Deserialize)]
struct WeightBenchmarkComponent {
    name: WeightBenchmarkComponentName,
    slope: Weight,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct WeightBenchmarkComponentName(String);

impl WeightBenchmarkComponentName {
    fn as_weight(&self, components: &HashMap<&str, Weight>) -> Weight {
        components[self.0.as_str()]
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
    let statistics = collect_data(data_folder_path, disable_filter);
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

fn weights(kind: WeightsKind, input_file: PathBuf, output_file: PathBuf) {
    macro_rules! add_weights {
        (
            benches = $benches:ident;
            let $name:ident {
                $( $field:ident $( : $underscore:tt )?, )+
            } = $e:expr;
        ) => {{
            let $name {
                $( $field $( : $underscore )?, )+
            } = $e;

            $(
                let field = add_weights!(@field $field $( : $underscore )?);
                $benches.extend(field);
            )+
        }};
        (@field $field:ident: _) => { None };
        (@field _phantom) => { None };
        (@field $field:ident) => {
            Some(GithubActionBenchmark {
                name: stringify!($field).to_string(),
                unit: "ns".to_string(),
                value: $field as u64 / 1_000,
                range: None,
                extra: None,
            })
        };
    }

    WEIGHTS_JSON.init(input_file);

    let schedule = runtime::Schedule::get();
    let mut benches = vec![];

    match kind {
        WeightsKind::HostFn => {
            add_weights! {
                benches = benches;
                let HostFnWeights {
                    _phantom,
                    alloc,
                    gr_gas_available,
                    gr_msg_id,
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
                    gr_send_init,
                    gr_send_push,
                    gr_send_push_per_byte,
                    gr_send_commit,
                    gr_send_commit_per_byte,
                    gr_reply_commit,
                    gr_reply_commit_per_byte,
                    gr_reply_push,
                    gr_reply_push_per_byte,
                    gr_reply_to,
                    gr_debug,
                    gr_exit_code,
                    gr_exit,
                    gr_leave,
                    gr_wait,
                    gr_wake,
                    gr_create_program_wgas,
                    gr_create_program_wgas_per_byte,
                    gas,
                } = schedule.host_fn_weights;
            }
        }
        WeightsKind::Instruction => {
            add_weights! {
                benches = benches;
                let InstructionWeights {
                    version: _,
                    i64const,
                    i64load,
                    i64store,
                    select,
                    r#if,
                    br,
                    br_if,
                    br_table,
                    br_table_per_entry,
                    call,
                    call_indirect,
                    call_indirect_per_param,
                    local_get,
                    local_set,
                    local_tee,
                    global_get,
                    global_set,
                    memory_current,
                    i64clz,
                    i64ctz,
                    i64popcnt,
                    i64eqz,
                    i64extendsi32,
                    i64extendui32,
                    i32wrapi64,
                    i64eq,
                    i64ne,
                    i64lts,
                    i64ltu,
                    i64gts,
                    i64gtu,
                    i64les,
                    i64leu,
                    i64ges,
                    i64geu,
                    i64add,
                    i64sub,
                    i64mul,
                    i64divs,
                    i64divu,
                    i64rems,
                    i64remu,
                    i64and,
                    i64or,
                    i64xor,
                    i64shl,
                    i64shrs,
                    i64shru,
                    i64rotl,
                    i64rotr,
                    _phantom,
                } = schedule.instruction_weights;
            }
        }
        WeightsKind::Memory => {
            add_weights! {
                benches = benches;
                let MemoryWeights {
                    initial_cost,
                    allocation_cost,
                    grow_cost,
                    load_cost,
                    _phantom,
                } = schedule.memory_weights;
            }
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
