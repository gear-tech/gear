// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use frame_support::{
    sp_runtime::{FixedPointNumber, FixedU128 as Fixed},
    weights::Weight,
};
use gear_utils::codegen::{LICENSE, format_with_rustfmt};
use heck::ToSnakeCase;
use indexmap::IndexMap;
use pallet_gear::Schedule;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File},
    path::PathBuf,
    str::FromStr,
};
use syn::{
    AngleBracketedGenericArguments, Fields, FnArg, GenericArgument, Generics, ImplItem, Item,
    ItemImpl, ItemStruct, Path, PathArguments, PathSegment, Type, TypePath,
    ext::IdentExt,
    visit::{self, Visit},
};
use tabled::{Style, builder::Builder};

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
    task_weights: IndexMap<String, Value>,
    instantiation_weights: IndexMap<String, Weight>,
    code_instrumentation_weights: IndexMap<String, Weight>,
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
        format!("{weight} ps")
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
                | "RentWeights"
                | "DbWeights"
                | "TaskWeights"
                | "InstantiationWeights"
                | "CodeInstrumentationWeights"
        ) {
            return;
        }

        let mut structure = node.clone();

        structure.attrs.retain(|attr| {
            attr.path()
                .segments
                .first()
                .filter(|segment| segment.ident == "doc")
                .is_some()
        });

        if structure_name == "Schedule" {
            structure.attrs.drain(1..);
        }

        structure.generics = Generics::default();

        if let Fields::Named(ref mut fields) = structure.fields
            && fields
                .named
                .last()
                .and_then(|field| field.ident.as_ref())
                .filter(|ident| *ident == "_phantom")
                .is_some()
        {
            fields.named.pop();
        }

        for field in structure.fields.iter_mut() {
            field.vis = syn::parse2(quote! { pub }).expect("infallible");

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

            field.attrs.retain(|attr| {
                attr.path()
                    .segments
                    .first()
                    .filter(|segment| segment.ident == "doc")
                    .is_some()
            });
        }

        self.structures.insert(structure_name, structure);

        visit::visit_item_struct(self, node);
    }
}

#[derive(Default)]
struct ImplementationVisitor {
    impls: Vec<ItemImpl>,
}

const TYPE_LIST: &[&str] = &[
    "InstructionCosts",
    "SyscallCosts",
    "MemoryCosts",
    "RentCosts",
    "InstantiationCosts",
    "InstrumentationCosts",
    "IoCosts",
    "DbCosts",
    "PagesCosts",
    "LazyPagesCosts",
];

impl ImplementationVisitor {
    fn find_from_impls(&mut self, node: &ItemImpl) -> bool {
        let mut implementation = node.clone();

        implementation.attrs.retain(|attr| {
            attr.path()
                .segments
                .first()
                .filter(|segment| segment.ident == "doc")
                .is_some()
        });

        implementation.generics = Generics::default();

        // first extract all the `*Costs` impls.
        if let Some((_, Path { segments, .. }, _)) = &mut implementation.trait_ {
            if let Some(PathSegment { ident, arguments }) = segments.first_mut()
                && *ident == "From"
                && let Type::Path(TypePath { path, .. }) = &mut *implementation.self_ty
            {
                let PathArguments::AngleBracketed(types) = arguments else {
                    unreachable!("unexpected From impl detected")
                };

                let Some(&mut GenericArgument::Type(ref mut ty)) = types.args.first_mut() else {
                    unreachable!("unexpected From impl detected")
                };

                if let Type::Path(TypePath { path, .. }) = ty
                    && let Some(PathSegment { arguments, .. }) = path.segments.first_mut()
                {
                    *arguments = PathArguments::None;
                }

                if let Some(PathSegment { ident, .. }) = path.segments.first_mut()
                    && TYPE_LIST.contains(&ident.to_string().as_str())
                {
                    let Some(ImplItem::Fn(from_fn)) = implementation.items.first_mut() else {
                        unreachable!("unexpected From impl detected")
                    };

                    let first_arg = from_fn.sig.inputs.first_mut().unwrap();
                    match first_arg {
                        FnArg::Typed(typed) => match &mut *typed.ty {
                            Type::Path(path) => {
                                path.path.segments.first_mut().unwrap().arguments =
                                    PathArguments::None;

                                self.impls.push(implementation);
                            }

                            _ => unreachable!("unexpected From impl detected"),
                        },
                        _ => unreachable!("unexpected From impl detected"),
                    }
                }
            }

            true
        } else {
            false
        }
    }

    fn find_process_costs(&mut self, node: &ItemImpl) {
        let mut implementation = node.clone();

        implementation.attrs.retain(|attr| {
            attr.path()
                .segments
                .first()
                .filter(|segment| segment.ident == "doc")
                .is_some()
        });

        implementation.generics = Generics::default();

        if let Type::Path(TypePath { path, .. }) = &mut *implementation.self_ty
            && let Some(PathSegment { arguments, ident }) = path.segments.first_mut()
        {
            *arguments = PathArguments::None;
            if *ident == "Schedule" {
                // only leave process_costs method
                implementation.items.retain_mut(|item| match item {
                    ImplItem::Fn(func) => func.sig.ident == "process_costs",
                    _ => false,
                });

                self.impls.push(implementation);
            }
        }
    }
}

impl<'ast> Visit<'ast> for ImplementationVisitor {
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if !self.find_from_impls(node) {
            self.find_process_costs(node);
        }

        visit::visit_item_impl(self, node);
    }
}

fn main() -> Result<()> {
    let Cli { command } = Cli::parse();

    match command {
        Commands::Dump { output_path, label } => {
            let writer = File::create(output_path)?;
            serde_json::to_writer_pretty(
                writer,
                &SerializableDump {
                    vara_schedule: Default::default(),
                    label,
                },
            )?;
        }
        Commands::Diff {
            display_units,
            before,
            after,
            runtime,
            kind,
        } => {
            let dump1: DeserializableDump = serde_json::from_str(&fs::read_to_string(before)?)?;

            let dump2: DeserializableDump = serde_json::from_str(&fs::read_to_string(after)?)?;

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
            let dump: DeserializableDump = serde_json::from_str(&fs::read_to_string(path)?)?;
            let raw_schedule = match runtime {
                Runtime::Vara => serde_json::to_value(dump.vara_schedule)?,
            };

            let file = syn::parse_file(&fs::read_to_string("pallets/gear/src/schedule.rs")?)?;

            let mut visitor = StructuresVisitor::default();
            visitor.visit_file(&file);

            let mut impl_visitor = ImplementationVisitor::default();
            impl_visitor.visit_file(&file);

            let impl_output = impl_visitor
                .impls
                .drain(..)
                .map(|item| Item::Impl(item).to_token_stream())
                .collect::<Vec<_>>();

            let mut declarations = vec![
                quote! {
                    #![allow(rustdoc::broken_intra_doc_links, missing_docs)]
                },
                quote! {
                    //! This is auto-generated module that contains cost schedule from
                    //! `pallets/gear/src/schedule.rs`.
                    //!
                    //! See `./scripts/weight-dump.sh` if you want to update it.
                },
                quote! {
                    use crate::costs::*;
                },
            ];

            for (structure_name, structure) in visitor.structures {
                let structure_ident = &structure.ident;

                let fields = structure.fields.iter().map(|field| {
                    let ty = &field.ty;
                    let type_name = quote! { #ty }.to_string().replace(' ', "");

                    let field_ident = field.ident.as_ref().unwrap();
                    let field_name = field_ident.unraw().to_string();

                    let structure_name_snake_case = structure_name.to_snake_case();
                    let value = match structure_name.as_str() {
                        "Schedule" => &raw_schedule[field_name],
                        "Limits"
                        | "InstructionWeights"
                        | "SyscallWeights"
                        | "MemoryWeights"
                        | "RentWeights"
                        | "DbWeights"
                        | "TaskWeights"
                        | "InstantiationWeights"
                        | "InstrumentationWeights" => {
                            &raw_schedule[structure_name_snake_case.as_str()][field_name]
                        }
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

                declarations.push(quote! {
                    #[derive(Debug, Clone)]
                    #structure
                });

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
                /// Represents the computational time and storage space required for an operation.
                #[derive(Debug, Clone, Copy)]
                pub struct Weight {
                    /// The weight of computational time used based on some reference hardware.
                    pub ref_time: u64,
                    /// The weight of storage space used by proof of validity.
                    pub proof_size: u64,
                }
            });

            declarations.push(quote! {
                impl Weight {
                    /// Return the reference time part of the weight.
                    #[doc(hidden)]
                    pub const fn ref_time(&self) -> u64 {
                        self.ref_time
                    }

                    /// Saturating [`Weight`] addition. Computes `self + rhs`, saturating at the numeric bounds of
                    /// all fields instead of overflowing.
                    #[doc(hidden)]
                    pub const fn saturating_add(&self, other: Self) -> Self {
                        Self {
                            ref_time: self.ref_time.saturating_add(other.ref_time),
                            proof_size: self.proof_size.saturating_add(other.proof_size)
                        }
                    }
                }
            });

            let output = declarations
                .into_iter()
                .chain(impl_output)
                .map(|stream| stream.to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            let formatted =
                format_with_rustfmt(format!("{}{output}", LICENSE.trim_start()).as_bytes());
            println!("{formatted}");
        }
    }

    Ok(())
}
