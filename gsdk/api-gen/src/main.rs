// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use parity_scale_codec::Decode;
use proc_macro2::TokenStream;
use std::{
    env, fs,
    io::{self, Write},
};
use subxt_codegen::{DerivesRegistry, RuntimeGenerator, TypeSubstitutes};
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

    {
        // TODO: customized code here. ( #2668 )
    }

    // Generate api.
    let runtime_types =
        LICENSE.trim_start().to_string() + &generate_runtime_types(metadata).to_string();
    io::stdout().write_all(runtime_types.as_bytes())?;

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
