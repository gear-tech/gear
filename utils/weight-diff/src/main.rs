// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Helper utility to track changes in weights between different branches.

use clap::{Parser, Subcommand, ValueEnum};
use frame_support::{
    sp_runtime::{FixedPointNumber, FixedU128 as Fixed},
    weights::Weight,
};
use indexmap::IndexMap;
use pallet_gear::Schedule;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tabled::{builder::Builder, Style};

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Takes weight information from compile time and saves it
    Dump {
        /// path to save the json file with both runtimes (gear and vara)
        #[arg(value_parser)]
        output_path: PathBuf,
        /// label to display tables with differences (e.g. branch, date of dump)
        #[arg(long)]
        label: Option<String>,
    },
    /// Compares two output files and generates the difference in tables
    Diff {
        /// path to json file #1
        #[arg(value_parser)]
        output_path1: PathBuf,
        /// path to json file #2
        #[arg(value_parser)]
        output_path2: PathBuf,
        /// what runtime to compare?
        #[arg(ignore_case = true, value_enum)]
        runtime: Runtime,
        /// for which weights to generate a table?
        #[arg(ignore_case = true, value_enum)]
        kind: WeightsKind,
        /// if present, displays the value in units
        #[arg(long)]
        display_units: bool,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum Runtime {
    Gear,
    Vara,
}

#[derive(Debug, Clone, ValueEnum)]
enum WeightsKind {
    Instruction,
    HostFn,
    Memory,
}

#[derive(Debug, Serialize)]
struct SerializableDump {
    gear_schedule: Schedule<gear_runtime::Runtime>,
    vara_schedule: Schedule<vara_runtime::Runtime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeserializableDump {
    gear_schedule: DeserializableSchedule,
    vara_schedule: DeserializableSchedule,
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeserializableSchedule {
    instruction_weights: IndexMap<String, serde_json::Value>,
    host_fn_weights: IndexMap<String, serde_json::Value>,
    memory_weights: IndexMap<String, serde_json::Value>,
}

impl DeserializableSchedule {
    fn instruction_weights(&self) -> IndexMap<String, u64> {
        let mut map = IndexMap::new();

        for (k, v) in self.instruction_weights.clone() {
            if k == "version" {
                continue;
            }

            if let Ok(v) = serde_json::from_value(v) {
                map.insert(k, v);
            }
        }

        map
    }

    fn host_fn_weights(&self) -> IndexMap<String, u64> {
        let mut map = IndexMap::new();

        for (k, v) in self.host_fn_weights.clone() {
            if let Ok(v) = serde_json::from_value::<Weight>(v) {
                map.insert(k, v.ref_time());
            }
        }

        map
    }

    fn memory_weights(&self) -> IndexMap<String, u64> {
        let mut map = IndexMap::new();

        for (k, v) in self.memory_weights.clone() {
            if let Ok(v) = serde_json::from_value::<Weight>(v) {
                map.insert(k, v.ref_time());
            }
        }

        map
    }
}

fn format_weight(weight: u64) -> String {
    if weight > 1_000_000_000 {
        format!(
            "{:.1?} ms",
            Fixed::saturating_from_rational(weight, 1_000_000_000).to_float(),
        )
    } else if weight > 1_000_000 {
        format!(
            "{:.1?} µs",
            Fixed::saturating_from_rational(weight, 1_000_000).to_float(),
        )
    } else if weight > 1_000 {
        format!(
            "{:.1?} ns",
            Fixed::saturating_from_rational(weight, 1_000).to_float(),
        )
    } else {
        format!("{} ps", weight)
    }
}

fn format_value(value: Option<u64>, display_units: bool) -> String {
    value
        .map(|v| {
            if display_units {
                format_weight(v)
            } else {
                format!("{v}")
            }
        })
        .unwrap_or_else(|| "N/A".into())
}

fn format_diff(value1: Option<u64>, value2: Option<u64>) -> String {
    value1
        .filter(|&a| a != 0)
        .zip(value2)
        .map(|(value1, value2)| {
            let (value1, value2) = (value1 as f64, value2 as f64);
            let percentage_diff = ((value1 / value2) - 1.0) * 100.0;
            format!("{percentage_diff:+.2}%")
        })
        .unwrap_or_else(|| "N/A".into())
}

fn main() {
    let Cli { command } = Cli::parse();

    match command {
        Commands::Dump { output_path, label } => {
            let writer = fs::File::create(output_path).unwrap();
            serde_json::to_writer_pretty(
                writer,
                &SerializableDump {
                    gear_schedule: Default::default(),
                    vara_schedule: Default::default(),
                    label,
                },
            )
            .unwrap();
        }
        Commands::Diff {
            display_units,
            output_path1,
            output_path2,
            runtime,
            kind,
        } => {
            let dump1: DeserializableDump =
                serde_json::from_str(&fs::read_to_string(output_path1).unwrap()).unwrap();

            let dump2: DeserializableDump =
                serde_json::from_str(&fs::read_to_string(output_path2).unwrap()).unwrap();

            let (schedule1, schedule2) = match runtime {
                Runtime::Gear => (dump1.gear_schedule, dump2.gear_schedule),
                Runtime::Vara => (dump1.vara_schedule, dump2.vara_schedule),
            };

            let (map1, map2) = match kind {
                WeightsKind::Instruction => (
                    schedule1.instruction_weights(),
                    schedule2.instruction_weights(),
                ),
                WeightsKind::HostFn => (schedule1.host_fn_weights(), schedule2.host_fn_weights()),
                WeightsKind::Memory => (schedule1.memory_weights(), schedule2.memory_weights()),
            };

            let mut result_map = IndexMap::new();

            for (key1, value1) in map1 {
                if let Some(&value2) = map2.get(&key1) {
                    result_map.insert(key1, (Some(value1), Some(value2)));
                } else {
                    result_map.insert(key1, (Some(value1), None));
                }
            }

            for (key2, value2) in map2 {
                if !result_map.contains_key(&key2) {
                    result_map.insert(key2, (None, Some(value2)));
                }
            }

            println!("Comparison table for {runtime:?} runtime");
            println!();

            let mut builder = Builder::default();
            builder.set_columns([
                "name".into(),
                dump1.label.unwrap_or_else(|| "value1".into()),
                dump2.label.unwrap_or_else(|| "value2".into()),
                "diff".into(),
            ]);

            for (key, (value1, value2)) in result_map {
                let val1 = format_value(value1, display_units);
                let val2 = format_value(value2, display_units);
                let diff = format_diff(value1, value2);

                builder.add_record([key, val1, val2, diff]);
            }

            let mut table = builder.build();
            table.with(Style::markdown());

            println!("{table}");
            println!();
        }
    }
}
