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
use frame_metadata::RuntimeMetadataPrefixed;
use heck::ToSnakeCase as _;
use parity_scale_codec::Decode;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use std::{
    env, fs,
    io::{self, Write},
};
use subxt_codegen::{DerivesRegistry, RuntimeGenerator, TypeSubstitutes};
use subxt_metadata::Metadata;
use syn::parse_quote;

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

    let output = {
        let metadata = Metadata::try_from(metadata).expect("Failed to convert metadata");

        let impls = generate_impls(&metadata);
        let runtime_types = generate_runtime_types(metadata);

        quote! {
            #runtime_types

            #impls
        }
    }
    .to_token_stream();

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

fn generate_runtime_types(metadata: Metadata) -> TokenStream {
    let generator = RuntimeGenerator::new(metadata);
    let runtime_types_mod = parse_quote!(
        pub mod runtime_types {}
    );

    let crate_path = Default::default();
    let mut derives = DerivesRegistry::new();
    derives.extend_for_all(
        [
            parse_quote!(Debug),
            parse_quote!(crate::gp::Encode),
            parse_quote!(crate::gp::Decode),
            parse_quote!(crate::gp::DecodeAsType),
        ],
        [],
    );

    generator
        .generate_runtime_types(
            runtime_types_mod,
            derives,
            TypeSubstitutes::with_default_substitutes(&crate_path),
            crate_path,
            true,
        )
        .expect("Failed to generate runtime types")
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
                        "validator_set" => "substrate_validator_set".into(),
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
        pub mod impls {
            use crate::metadata::Event;

            impl subxt::events::RootEvent for Event {
                fn root_event(
                    pallet_bytes: &[u8],
                    pallet_name: &str,
                    pallet_ty: u32,
                    metadata: &subxt::Metadata
                ) -> Result<Self, subxt::Error> {
                    use subxt::metadata::DecodeWithMetadata;

                    #( #root_event_if_arms )*

                    Err(subxt::ext::scale_decode::Error::custom(
                        format!("Pallet name '{}' not found in root Event enum", pallet_name)
                    ).into())
                }
            }
        }

        pub mod exports {
            use crate::metadata::runtime_types;

            #( #exports )*
        }
    }
}
