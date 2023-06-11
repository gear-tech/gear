// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
use parity_scale_codec::Decode;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Write},
};
use subxt_codegen::{DerivesRegistry, RuntimeGenerator, TypeSubstitutes};
use syn::{parse_quote, Fields, ItemEnum, ItemImpl, ItemMod, Variant};

const RUNTIME_WASM: &str = "RUNTIME_WASM";
const USAGE: &str = r#"
Usage: RUNTIME_WASM=<path> generate-client-api
"#;
const LICENSE: &str = r#"
// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
#[allow(rustdoc::broken_intra_doc_links)] //subxt-codegen produces incorrect docs
"#;

fn main() -> Result<()> {
    color_eyre::install()?;

    if env::args().len() != 1 {
        println!("{}", USAGE.trim());
        return Ok(());
    }

    // Get the metadata of vara runtime.
    let encoded = metadata();
    if encoded.len() < 4 {
        panic!("Invalid metadata, doesn't even have enough bytes for the magic number.");
    }

    // NOTE: [4..] here for removing the magic number.
    let metadata = <RuntimeMetadataPrefixed as Decode>::decode(&mut encoded[4..].as_ref())
        .expect("decode metadata failed");

    // Customized code here.
    let calls = generate_calls(&metadata.1);
    let types = generate_runtime_types(metadata);

    let output = quote! {
        #types

        #calls
    }
    .to_token_stream();

    // Generate api.
    io::stdout().write_all((LICENSE.trim_start().to_string() + &output.to_string()).as_bytes())?;

    Ok(())
}

/// Get the metadata of vara runtime.
fn metadata() -> Vec<u8> {
    use gear_runtime_interface as gear_ri;
    use sc_executor::WasmExecutionMethod;
    use sc_executor_common::runtime_blob::RuntimeBlob;

    // 1. Get the wasm binary of `RUNTIME_WASM`.
    let path = env::var(RUNTIME_WASM).expect("Missing RUNTIME_WASM env var.");
    let code = fs::read(path).expect("Failed to read runtime wasm");

    // 2. Create wasm executor.
    let executor = sc_executor::WasmExecutor::<(
        gear_ri::gear_ri::HostFunctions,
        sp_io::SubstrateHostFunctions,
    )>::new(WasmExecutionMethod::Interpreted, Some(1024), 8, None, 2);

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

fn generate_runtime_types(metadata: RuntimeMetadataPrefixed) -> TokenStream {
    let generator = RuntimeGenerator::new(metadata);
    let runtime_types_mod = parse_quote!(
        pub mod runtime_types {}
    );

    let crate_path = Default::default();

    // TODO: extend `Copy` for Ids and Hashes. ( #2668 )
    let derives = DerivesRegistry::new(&crate_path);
    generator
        .generate_runtime_types(
            runtime_types_mod,
            derives,
            TypeSubstitutes::new(&crate_path),
            crate_path,
            true,
        )
        .expect("Failed to generate runtime types")
}

/// Generate a table for the calls.
fn generate_calls(wrapper: &RuntimeMetadata) -> ItemMod {
    let metadata = if let RuntimeMetadata::V14(v14) = wrapper {
        v14
    } else {
        panic!("Unsupported metadata version, only support v14.");
    };

    // Call table.
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pallet in metadata.pallets.clone().into_iter() {
        let pallet_name = pallet.name.clone();
        let calls = pallet.calls.map(|call| {
            let scale_info::TypeDef::Variant(variant) = &metadata.types.resolve(call.ty.id).expect("Unknown calls").type_def else {
                panic!("Invalid call type {call:?}");
            };

            variant
                .variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect::<Vec<_>>()
        });

        if let Some(calls) = calls {
            table.insert(pallet_name, calls);
        }
    }

    let (mut ie, mut ii): (Vec<ItemEnum>, Vec<ItemImpl>) = (vec![], vec![]);
    for (pallet, calls) in table {
        let pallet_ident = Ident::new(&pallet, Span::call_site());
        let call_var = calls
            .iter()
            .map(|call| {
                // Convert snake case call name to camel case
                let var = call
                    .split("_")
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

        let doc = format!("Calls of pallet `{}`.", pallet);
        ie.push(parse_quote! {
            #[doc = #doc]
            pub enum #pallet_ident {
                #(#call_var,)*
            }
        });

        ii.push(parse_quote! {
            impl CallInfo for #pallet_ident {
                const PALLET: &'static str = #pallet;

                fn call_name(&self) -> &str {
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
                fn call_name(&self) -> &str;
            }

            #(
                #ie

                #ii
            )*
        }
    }
}
