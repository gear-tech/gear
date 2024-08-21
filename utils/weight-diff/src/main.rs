// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use gear_utils::codegen::{format_with_rustfmt, LICENSE};
use indexmap::IndexMap;
use pallet_gear::Schedule;
use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, str::FromStr};
use syn::{
    ext::IdentExt,
    visit::{self, Visit},
    AngleBracketedGenericArguments, Fields, Generics, ItemStruct, PathArguments, Type, TypePath,
};
use tabled::{builder::Builder, Style};

/// Utility for working with weights
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
        before: PathBuf,
        /// path to json file #2
        #[arg(value_parser)]
        after: PathBuf,
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
    /// Creates lightweight scheduler with weights from the given json file
    Codegen {
        /// path to json file
        #[arg(value_parser)]
        path: PathBuf,
        /// what runtime to use as source?
        #[arg(ignore_case = true, value_enum)]
        runtime: Runtime,
    },
    /// Creates code to initialize scheduler with weights from lightweight scheduler
    GtestCodegen,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum Runtime {
    Vara,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum WeightsKind {
    Instruction,
    HostFn,
    Memory,
}

#[derive(Debug, Serialize)]
struct SerializableDump {
    vara_schedule: Schedule<vara_runtime::Runtime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeserializableDump {
    vara_schedule: DeserializableSchedule,
    label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeserializableSchedule {
    limits: IndexMap<String, Value>,
    instruction_weights: IndexMap<String, Value>,
    syscall_weights: IndexMap<String, Weight>,
    memory_weights: IndexMap<String, Weight>,
    rent_weights: IndexMap<String, Weight>,
    db_weights: IndexMap<String, Value>,
    instantiation_weights: IndexMap<String, Weight>,
    #[serde(flatten)]
    other_fields: IndexMap<String, Weight>,
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

    fn syscall_weights(&self) -> IndexMap<String, u64> {
        let mut map = IndexMap::new();

        for (k, v) in self.syscall_weights.clone() {
            map.insert(k, v.ref_time());
        }

        map
    }

    fn memory_weights(&self) -> IndexMap<String, u64> {
        let mut map = IndexMap::new();

        for (k, v) in self.memory_weights.clone() {
            map.insert(k, v.ref_time());
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

fn format_diff(before: Option<u64>, after: Option<u64>) -> String {
    let after = after.filter(|&x| x != 0);
    if let (Some(before), Some(after)) = (before, after) {
        let (before, after) = (before as f64, after as f64);
        let percentage_diff = (1.0 - before / after) * 100.0;
        format!("{percentage_diff:+.2}%")
    } else {
        "N/A".to_string()
    }
}

#[derive(Default)]
struct StructuresVisitor {
    structures: IndexMap<String, ItemStruct>,
}

impl<'ast> Visit<'ast> for StructuresVisitor {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let structure_name = node.ident.to_string();
        if !matches!(
            structure_name.as_str(),
            "Schedule"
                | "Limits"
                | "InstructionWeights"
                | "SyscallWeights"
                | "MemoryWeights"
                | "InstantiationWeights"
                | "RentWeights"
                | "DbWeights"
        ) {
            return;
        }

        let mut structure = node.clone();

        structure.attrs.clear();
        structure.generics = Generics::default();

        if let Fields::Named(ref mut fields) = structure.fields {
            let last_ident = fields
                .named
                .last()
                .and_then(|field| field.ident.as_ref().map(|ident| ident.to_string()));
            if last_ident == Some(String::from("_phantom")) {
                fields.named.pop();
            }
        }

        for field in structure.fields.iter_mut() {
            field.vis = syn::parse2(quote! { pub }).unwrap();

            if let Type::Path(TypePath { path, .. }) = &mut field.ty {
                for segment in path.segments.iter_mut() {
                    if let PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                        args,
                        ..
                    }) = &mut segment.arguments
                    {
                        let token_stream = quote! { #args };
                        if token_stream.to_string() == "T" {
                            segment.arguments = PathArguments::None;
                        }
                    }
                }
            }
            field.attrs.clear();
        }

        self.structures.insert(structure_name, structure);

        visit::visit_item_struct(self, node);
    }
}

fn main() {
    let Cli { command } = Cli::parse();

    match command {
        Commands::Dump { output_path, label } => {
            let writer = fs::File::create(output_path).unwrap();
            serde_json::to_writer_pretty(
                writer,
                &SerializableDump {
                    vara_schedule: Default::default(),
                    label,
                },
            )
            .unwrap();
        }
        Commands::Diff {
            display_units,
            before,
            after,
            runtime,
            kind,
        } => {
            let dump1: DeserializableDump =
                serde_json::from_str(&fs::read_to_string(before).unwrap()).unwrap();

            let dump2: DeserializableDump =
                serde_json::from_str(&fs::read_to_string(after).unwrap()).unwrap();

            let (schedule1, schedule2) = match runtime {
                Runtime::Vara => (dump1.vara_schedule, dump2.vara_schedule),
            };

            let (map1, map2) = match kind {
                WeightsKind::Instruction => (
                    schedule1.instruction_weights(),
                    schedule2.instruction_weights(),
                ),
                WeightsKind::HostFn => (schedule1.syscall_weights(), schedule2.syscall_weights()),
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

            println!("Comparison table for {runtime:?} runtime for {kind:?}");
            println!();

            let mut builder = Builder::default();
            builder.set_columns([
                "name".into(),
                dump1.label.unwrap_or_else(|| "before".into()),
                dump2.label.unwrap_or_else(|| "after".into()),
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
        Commands::Codegen { path, runtime } => {
            let dump: DeserializableDump =
                serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            let raw_schedule = match runtime {
                Runtime::Vara => serde_json::to_value(dump.vara_schedule).unwrap(),
            };

            let file =
                syn::parse_file(&fs::read_to_string("pallets/gear/src/schedule.rs").unwrap())
                    .unwrap();

            let mut visitor = StructuresVisitor::default();
            visitor.visit_file(&file);

            let mut declarations = vec![quote! {
                //! This is auto-generated module that contains cost schedule from
                //! `pallets/gear/src/schedule.rs`.
                //!
                //! See `./scripts/weight-dump.sh` if you want to update it.
            }];

            for (structure_name, structure) in visitor.structures {
                let structure_ident = &structure.ident;

                let fields = structure.fields.iter().map(|field| {
                    let ty = &field.ty;
                    let type_name = quote! { #ty }.to_string().replace(' ', "");

                    let field_ident = field.ident.as_ref().unwrap();
                    let field_name = field_ident.unraw().to_string();

                    let value = match structure_name.as_str() {
                        "Schedule" => &raw_schedule[field_name],
                        "Limits" => &raw_schedule["limits"][field_name],
                        "InstructionWeights" => &raw_schedule["instruction_weights"][field_name],
                        "SyscallWeights" => &raw_schedule["syscall_weights"][field_name],
                        "MemoryWeights" => &raw_schedule["memory_weights"][field_name],
                        "InstantiationWeights" => {
                            &raw_schedule["instantiation_weights"][field_name]
                        }
                        "RentWeights" => &raw_schedule["rent_weights"][field_name],
                        "DbWeights" => &raw_schedule["db_weights"][field_name],
                        _ => &raw_schedule,
                    };

                    let default_value = match type_name.as_str() {
                        "Weight" => {
                            let ref_time =
                                TokenStream::from_str(&value["ref_time"].to_string()).unwrap();
                            let proof_size =
                                TokenStream::from_str(&value["proof_size"].to_string()).unwrap();
                            quote! {
                                Weight {
                                    ref_time: #ref_time,
                                    proof_size: #proof_size,
                                }
                            }
                        }
                        "Option<u32>" => {
                            let value = TokenStream::from_str(&value.to_string()).unwrap();
                            quote! { Some(#value) }
                        }
                        "u32" | "u16" => {
                            let value = TokenStream::from_str(&value.to_string()).unwrap();
                            quote! { #value }
                        }
                        _ => quote! { #ty::default() },
                    };

                    quote! {
                        #field_ident: #default_value,
                    }
                });

                declarations.push(quote! { #structure });
                declarations.push(quote! {
                    impl Default for #structure_ident {
                        fn default() -> Self {
                            Self {
                                #(#fields)*
                            }
                        }
                    }
                });
            }

            declarations.push(quote! {
                pub struct Weight {
                    pub ref_time: u64,
                    pub proof_size: u64,
                }
            });

            let output = declarations
                .into_iter()
                .map(|stream| stream.to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            let formatted =
                format_with_rustfmt(format!("{}{output}", LICENSE.trim_start()).as_bytes());
            println!("{formatted}");
        }

        Commands::GtestCodegen => {
            let mut declarations = vec![quote! {
                //! This is auto-generated module that contains costs constructors
                //! `pallets/gear/src/schedule.rs`.
                //!
                //! See `./scripts/weight-dump.sh` if you want to update it.

                use core_processor::configs::{ExtCosts, InstantiationCosts, ProcessCosts, RentCosts};
                use gear_core::costs::SyscallCosts;
                use gear_lazy_pages_common::LazyPagesCosts;
                use gear_wasm_instrument::gas_metering::{InstantiationWeights, MemoryWeights, SyscallWeights, Schedule};

            }];

            // LazyPagesCosts
            declarations.push(quote! {
                pub fn lazy_pages_costs(val: &MemoryWeights) -> LazyPagesCosts {
                    LazyPagesCosts {
                        host_func_read: val.lazy_pages_host_func_read.ref_time.into(),
                        host_func_write: val.lazy_pages_host_func_write.ref_time
                            .saturating_add(val.upload_page_data.ref_time)
                            .into(),
                        host_func_write_after_read: val.lazy_pages_host_func_write_after_read.ref_time
                            .saturating_add(val.upload_page_data.ref_time)
                            .into(),
                        load_page_storage_data: val.load_page_data.ref_time
                            .saturating_add(val.parachain_read_heuristic.ref_time)
                            .into(),
                        signal_read: val.lazy_pages_signal_read.ref_time.into(),
                        signal_write: val.lazy_pages_signal_write.ref_time
                            .saturating_add(val.upload_page_data.ref_time)
                            .into(),
                        signal_write_after_read: val.lazy_pages_signal_write_after_read.ref_time
                            .saturating_add(val.upload_page_data.ref_time)
                            .into(),
                    }
                }
            });

            //InstantiationCosts
            declarations.push(quote! {
                pub fn instantiation_costs(val: &InstantiationWeights) -> InstantiationCosts {
                    InstantiationCosts {
                        code_section_per_byte: val.code_section_per_byte.ref_time.into(),
                        data_section_per_byte: val.data_section_per_byte.ref_time.into(),
                        global_section_per_byte: val.global_section_per_byte.ref_time.into(),
                        table_section_per_byte: val.table_section_per_byte.ref_time.into(),
                        element_section_per_byte: val.element_section_per_byte.ref_time.into(),
                        type_section_per_byte: val.type_section_per_byte.ref_time.into()
                    }
                }

            });

            // SyscallCosts
            declarations.push(quote! {
                pub fn syscall_costs(val: &SyscallWeights) -> SyscallCosts {
                    SyscallCosts {
                        alloc: val.alloc.ref_time.into(),
                        free: val.free.ref_time.into(),
                        free_range: val.free_range.ref_time.into(),
                        free_range_per_page: val.free_range_per_page.ref_time.into(),
                        gr_reserve_gas: val.gr_reserve_gas.ref_time.into(),
                        gr_unreserve_gas: val.gr_unreserve_gas.ref_time.into(),
                        gr_system_reserve_gas: val.gr_system_reserve_gas.ref_time.into(),
                        gr_gas_available: val.gr_gas_available.ref_time.into(),
                        gr_message_id: val.gr_message_id.ref_time.into(),
                        gr_program_id: val.gr_program_id.ref_time.into(),
                        gr_source: val.gr_source.ref_time.into(),
                        gr_value: val.gr_value.ref_time.into(),
                        gr_value_available: val.gr_value_available.ref_time.into(),
                        gr_size: val.gr_size.ref_time.into(),
                        gr_read: val.gr_read.ref_time.into(),
                        gr_read_per_byte: val.gr_read_per_byte.ref_time.into(),
                        gr_env_vars: val.gr_env_vars.ref_time.into(),
                        gr_block_height: val.gr_block_height.ref_time.into(),
                        gr_block_timestamp: val.gr_block_timestamp.ref_time.into(),
                        gr_random: val.gr_random.ref_time.into(),
                        gr_reply_deposit: val.gr_reply_deposit.ref_time.into(),
                        gr_send: val.gr_send.ref_time.into(),
                        gr_send_per_byte: val.gr_send_per_byte.ref_time.into(),
                        gr_send_wgas: val.gr_send_wgas.ref_time.into(),
                        gr_send_wgas_per_byte: val.gr_send_wgas_per_byte.ref_time.into(),
                        gr_send_init: val.gr_send_init.ref_time.into(),
                        gr_send_push: val.gr_send_push.ref_time.into(),
                        gr_send_push_per_byte: val.gr_send_push_per_byte.ref_time.into(),
                        gr_send_commit: val.gr_send_commit.ref_time.into(),
                        gr_send_commit_wgas: val.gr_send_commit_wgas.ref_time.into(),
                        gr_reservation_send: val.gr_reservation_send.ref_time.into(),
                        gr_reservation_send_per_byte: val.gr_reservation_send_per_byte.ref_time.into(),
                        gr_reservation_send_commit: val.gr_reservation_send_commit.ref_time.into(),
                        gr_reply_commit: val.gr_reply_commit.ref_time.into(),
                        gr_reply_commit_wgas: val.gr_reply_commit_wgas.ref_time.into(),
                        gr_reservation_reply: val.gr_reservation_reply.ref_time.into(),
                        gr_reservation_reply_per_byte: val.gr_reservation_reply_per_byte.ref_time.into(),
                        gr_reservation_reply_commit: val.gr_reservation_reply_commit.ref_time.into(),
                        gr_reply_push: val.gr_reply_push.ref_time.into(),
                        gr_reply: val.gr_reply.ref_time.into(),
                        gr_reply_per_byte: val.gr_reply_per_byte.ref_time.into(),
                        gr_reply_wgas: val.gr_reply_wgas.ref_time.into(),
                        gr_reply_wgas_per_byte: val.gr_reply_wgas_per_byte.ref_time.into(),
                        gr_reply_push_per_byte: val.gr_reply_push_per_byte.ref_time.into(),
                        gr_reply_to: val.gr_reply_to.ref_time.into(),
                        gr_signal_code: val.gr_signal_code.ref_time.into(),
                        gr_signal_from: val.gr_signal_from.ref_time.into(),
                        gr_reply_input: val.gr_reply_input.ref_time.into(),
                        gr_reply_input_wgas: val.gr_reply_input_wgas.ref_time.into(),
                        gr_reply_push_input: val.gr_reply_push_input.ref_time.into(),
                        gr_reply_push_input_per_byte: val.gr_reply_push_input_per_byte.ref_time.into(),
                        gr_send_input: val.gr_send_input.ref_time.into(),
                        gr_send_input_wgas: val.gr_send_input_wgas.ref_time.into(),
                        gr_send_push_input: val.gr_send_push_input.ref_time.into(),
                        gr_send_push_input_per_byte: val.gr_send_push_input_per_byte.ref_time.into(),
                        gr_debug: val.gr_debug.ref_time.into(),
                        gr_debug_per_byte: val.gr_debug_per_byte.ref_time.into(),
                        gr_reply_code: val.gr_reply_code.ref_time.into(),
                        gr_exit: val.gr_exit.ref_time.into(),
                        gr_leave: val.gr_leave.ref_time.into(),
                        gr_wait: val.gr_wait.ref_time.into(),
                        gr_wait_for: val.gr_wait_for.ref_time.into(),
                        gr_wait_up_to: val.gr_wait_up_to.ref_time.into(),
                        gr_wake: val.gr_wake.ref_time.into(),
                        gr_create_program: val.gr_create_program.ref_time.into(),
                        gr_create_program_payload_per_byte: val.gr_create_program_payload_per_byte.ref_time.into(),
                        gr_create_program_salt_per_byte: val.gr_create_program_salt_per_byte.ref_time.into(),
                        gr_create_program_wgas: val.gr_create_program_wgas.ref_time.into(),
                        gr_create_program_wgas_payload_per_byte: val.gr_create_program_wgas_payload_per_byte.ref_time.into(),
                        gr_create_program_wgas_salt_per_byte: val.gr_create_program_wgas_salt_per_byte.ref_time.into(),
                    }
                }
            });

            // process_costs and block_config()
            let vara_schedule: Schedule<vara_runtime::Runtime> = Default::default();

            let process_costs = vara_schedule.process_costs();

            let instrumentation = process_costs.instrumentation.cost_for_one();
            let instrumentation_per_byte = process_costs.instrumentation_per_byte.cost_for_one();
            let load_allocations_per_interval =
                process_costs.load_allocations_per_interval.cost_for_one();
            declarations.push(quote! {
                pub fn process_costs(schedule: &Schedule) -> ProcessCosts {
                    ProcessCosts {
                        ext: ExtCosts {
                            rent: RentCosts {
                                waitlist: schedule.rent_weights.waitlist.ref_time.into(),
                                dispatch_stash: schedule.rent_weights.dispatch_stash.ref_time.into(),
                                reservation: schedule.rent_weights.reservation.ref_time.into()
                            },
                            syscalls: syscall_costs(&schedule.syscall_weights),
                            mem_grow: schedule.memory_weights.mem_grow.ref_time.into(),
                            mem_grow_per_page: schedule.memory_weights.mem_grow_per_page.ref_time.into(),
                        },
                        lazy_pages: lazy_pages_costs(&schedule.memory_weights),
                        read: schedule.db_weights.read.ref_time.into(),
                        write: schedule.db_weights.write.ref_time.into(),
                        read_per_byte: schedule.db_weights.read_per_byte.ref_time.into(),
                        instrumentation: #instrumentation.into(),
                        instrumentation_per_byte: #instrumentation_per_byte.into(),
                        instantiation_costs: instantiation_costs(&schedule.instantiation_weights),
                        load_allocations_per_interval: #load_allocations_per_interval.into()
                    }
                }
            });

            let output = declarations
                .into_iter()
                .map(|stream| stream.to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            let formatted =
                format_with_rustfmt(format!("{}{output}", LICENSE.trim_start()).as_bytes());
            println!("{formatted}");
        }
    }
}
