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
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed, RuntimeMetadataV14};
use parity_scale_codec::Decode;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
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
        let v14 = if let RuntimeMetadata::V14(v14) = metadata.1 {
            v14
        } else {
            panic!("Runtime metadata is not v14");
        };

        let runtime_types = generate_runtime_types(v14);

        quote! {
            #runtime_types
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

fn generate_runtime_types(metadata: RuntimeMetadataV14) -> TokenStream {
    let metadata = Metadata::try_from(metadata).expect("Failed to convert metadata");
    let generator = RuntimeGenerator::new(metadata);
    let runtime_types_mod = parse_quote!(
        pub mod runtime_types {}
    );

    let crate_path = Default::default();
    let derives = DerivesRegistry::new();
    generator
        .generate_runtime_types(
            runtime_types_mod,
            derives,
            TypeSubstitutes::new(),
            crate_path,
            true,
        )
        .expect("Failed to generate runtime types")
}

fn _generate_impls(_metadata: RuntimeMetadataV14) {}
