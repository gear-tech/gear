// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use color_eyre::eyre::Result;
use gear_utils::codegen::LICENSE;
use heck::ToSnakeCase as _;
use parity_scale_codec::Decode;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Write},
};
use subxt_codegen::{CodegenBuilder, Metadata};
use syn::{Fields, ItemEnum, ItemImpl, ItemMod, Variant, parse_quote};

const RUNTIME_WASM: &str = "RUNTIME_WASM";
const PRINT_SCALE: &str = "PRINT_SCALE";
const USAGE: &str = r#"
Usage: RUNTIME_WASM=<path> generate-client-api
"#;

fn main() -> Result<()> {
    color_eyre::install()?;

    if env::args().len() < 1 {
        println!("{}", USAGE.trim());
        return Ok(());
    }

    // Get the metadata of vara runtime.
    let encoded = metadata();
    if encoded.len() < 4 {
        panic!("Invalid metadata, doesn't even have enough bytes for the magic number.");
    }

    if env::var(PRINT_SCALE).is_ok() {
        io::stdout().write_all(("0x".to_owned() + &hex::encode(&encoded[4..])).as_bytes())?;
        return Ok(());
    }

    // NOTE: [4..] here for removing the magic number.
    let metadata =
        Metadata::decode(&mut encoded[4..].as_ref()).expect("Failed to convert metadata");
    let calls = generate_calls(&metadata);
    let storage = generate_storage(&metadata);
    let impls = generate_impls(&metadata);
    let types = generate_runtime_types(metadata);

    let output = quote! {
        #types

        #calls

        #storage

        #impls
    }
    .to_token_stream();

    io::stdout().write_all((LICENSE.trim_start().to_string() + &output.to_string()).as_bytes())?;
    Ok(())
}

/// Get the metadata of vara runtime.
fn metadata() -> Vec<u8> {
    use gear_runtime_interface as gear_ri;
    use sc_executor::{WasmExecutionMethod, WasmtimeInstantiationStrategy};
    use sc_executor_common::runtime_blob::RuntimeBlob;

    // 1. Get the wasm binary of `RUNTIME_WASM`.
    let path = env::var(RUNTIME_WASM).expect("Missing RUNTIME_WASM env var.");
    let code = fs::read(path).expect("Failed to read runtime wasm");

    let heap_pages =
        sc_executor_common::wasm_runtime::HeapAllocStrategy::Static { extra_pages: 1024 };

    // 2. Create wasm executor.
    let executor = sc_executor::WasmExecutor::<(
        gear_ri::gear_ri::HostFunctions,
        sp_io::SubstrateHostFunctions,
    )>::builder()
    .with_execution_method(WasmExecutionMethod::Compiled {
        instantiation_strategy: WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
    })
    .with_onchain_heap_alloc_strategy(heap_pages)
    .with_offchain_heap_alloc_strategy(heap_pages)
    .with_max_runtime_instances(8)
    .with_runtime_cache_size(2)
    .build();

    // 3. Extract metadata.
    executor
        .uncached_call(
            RuntimeBlob::uncompress_if_needed(&code).expect("Invalid runtime code."),
            &mut sp_io::TestExternalities::default().ext(),
            true,
            "Metadata_metadata",
            &[],
        )
        .expect("Failed to extract runtime metadata")
        .to_vec()
}

fn generate_runtime_types(metadata: Metadata) -> TokenStream {
    let mut builder = CodegenBuilder::new();
    builder.set_additional_global_derives(vec![
        parse_quote!(Debug),
        parse_quote!(crate::gp::Encode),
        parse_quote!(crate::gp::Decode),
        parse_quote!(crate::gp::DecodeAsType),
    ]);
    builder.set_additional_global_attributes(vec![
        parse_quote!(#[decode_as_type(crate_path = "::subxt::ext::scale_decode")]),
        parse_quote!(#[codec(crate = ::subxt::ext::codec)]),
    ]);

    for ty in [
        parse_quote!(gprimitives::CodeId),
        parse_quote!(gprimitives::MessageId),
        parse_quote!(gprimitives::ActorId),
        parse_quote!(gprimitives::ReservationId),
    ] {
        builder.add_derives_for_type(ty, [parse_quote!(Copy)], true);
    }

    builder.runtime_types_only();
    builder.disable_default_derives();
    builder.set_target_module(parse_quote! {
        pub mod runtime_types {}
    });
    builder
        .generate(metadata)
        .expect("Failed to generate runtime types")
}

/// Generate a table for the calls.
fn generate_calls(metadata: &Metadata) -> ItemMod {
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pallet in metadata.pallets() {
        let pallet_name = pallet.name();
        let calls = pallet.call_variants().map(|calls| {
            calls
                .iter()
                .map(|call| call.name.clone())
                .collect::<Vec<_>>()
        });

        if let Some(calls) = calls {
            table.insert(pallet_name.into(), calls);
        }
    }

    let (mut ie, mut ii): (Vec<ItemEnum>, Vec<ItemImpl>) = (vec![], vec![]);
    for (pallet, calls) in table {
        let pallet_ident = Ident::new(&(pallet.clone() + "Call"), Span::call_site());
        let call_var = calls
            .iter()
            .map(|call| {
                // Convert snake case call name to camel case
                let var = call
                    .split('_')
                    .map(|w| {
                        let mut c = w.chars();
                        c.next()
                            .expect("Invalid string in call name")
                            .to_uppercase()
                            .chain(c)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .concat();

                let ident = Ident::new(&var, Span::call_site());
                Variant {
                    attrs: vec![],
                    ident,
                    fields: Fields::Unit,
                    discriminant: None,
                }
            })
            .collect::<Vec<Variant>>();

        let doc = format!("Calls of pallet `{pallet}`.");
        ie.push(parse_quote! {
            #[doc = #doc]
            pub enum #pallet_ident {
                #(#call_var,)*
            }
        });

        ii.push(parse_quote! {
            impl CallInfo for #pallet_ident {
                const PALLET: &'static str = #pallet;

                fn call_name(&self) -> &'static str {
                    match self {
                        #(
                            Self::#call_var => #calls,
                        )*
                    }
                }
            }
        });
    }

    parse_quote! {
        pub mod calls {
            /// Show the call info.
            pub trait CallInfo {
                const PALLET: &'static str;

                /// returns call name.
                fn call_name(&self) -> &'static str;
            }

            #(
                #ie

                #ii
            )*
        }
    }
}

/// Generate a table for the calls.
fn generate_storage(metadata: &Metadata) -> ItemMod {
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pallet in metadata.pallets() {
        let pallet_name = pallet.name();

        let storage = pallet.storage().map(|storage| {
            storage
                .entries()
                .iter()
                .map(|entry| entry.name().into())
                .collect::<Vec<_>>()
        });

        if let Some(storage) = storage {
            table.insert(pallet_name.into(), storage);
        }
    }

    let (mut ie, mut ii): (Vec<ItemEnum>, Vec<ItemImpl>) = (vec![], vec![]);
    for (pallet, storage) in table {
        let pallet_ident = Ident::new(&(pallet.clone() + "Storage"), Span::call_site());
        let storage_var = storage
            .iter()
            .map(|storage| {
                let ident = Ident::new(storage, Span::call_site());
                Variant {
                    attrs: vec![],
                    ident,
                    fields: Fields::Unit,
                    discriminant: None,
                }
            })
            .collect::<Vec<Variant>>();

        let doc = format!("Storage of pallet `{pallet}`.");
        ie.push(parse_quote! {
            #[doc = #doc]
            pub enum #pallet_ident {
                #(#storage_var,)*
            }
        });

        ii.push(parse_quote! {
            impl StorageInfo for #pallet_ident {
                const PALLET: &'static str = #pallet;

                fn storage_name(&self) -> &'static str {
                    match self {
                        #(
                            Self::#storage_var => #storage,
                        )*
                    }
                }
            }
        });
    }

    parse_quote! {
        pub mod storage {
             /// Show the call info.
             pub trait StorageInfo {
                 const PALLET: &'static str;

                 /// returns call name.
                 fn storage_name(&self) -> &'static str;
             }

             #(
                 #ie

                 #ii
             )*
        }
    }
}

fn generate_impls(metadata: &Metadata) -> TokenStream {
    let mut root_event_if_arms = Vec::new();
    let mut exports = Vec::new();

    for p in metadata.pallets() {
        let variant_name_str = p.name();
        let variant_name = format_ident!("{}", variant_name_str);
        let mod_name = format_ident!("{}", variant_name_str.to_string().to_snake_case());

        if p.event_ty_id().is_some() {
            let ia = quote! {
                if pallet_name == #variant_name_str {
                    return Ok(Event::#variant_name(crate::metadata::#mod_name::Event::decode_with_metadata(
                        &mut &*pallet_bytes,
                        pallet_ty,
                        metadata
                    )?));
                }
            };
            root_event_if_arms.push(ia);

            let export = {
                let pallet_name = variant_name_str.to_snake_case();
                let pallet = format_ident!(
                    "{}",
                    match pallet_name.as_str() {
                        "system" => "frame_system".into(),
                        "fellowship_collective" => "pallet_ranked_collective".into(),
                        "fellowship_referenda" => "pallet_referenda".into(),
                        "staking_rewards" => "pallet_gear_staking_rewards".into(),
                        _ => "pallet_".to_string() + &pallet_name,
                    }
                );

                let export = match pallet_name.as_str() {
                    "staking" => quote! {
                        pub use super::runtime_types::#pallet::pallet::pallet::Event;
                    },
                    "referenda" => quote! {
                        pub use super::runtime_types::#pallet::pallet::Event1 as Event;
                    },
                    "fellowship_referenda" => quote! {
                        pub use super::runtime_types::#pallet::pallet::Event2 as Event;
                    },
                    _ => quote! {
                        pub use super::runtime_types::#pallet::pallet::Event;
                    },
                };

                let name = format_ident!("{}", pallet_name);
                quote! {
                    pub mod #name {
                        #export
                    }
                }
            };
            exports.push(export);
        }
    }

    quote! {
        pub mod exports {
            use crate::metadata::runtime_types;

            #( #exports )*
        }
    }
}
