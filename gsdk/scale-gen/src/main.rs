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

use std::{env, fs};

use color_eyre::eyre::{bail, Context, ContextCompat, Result};
use gear_runtime_interface as gear_ri;
use parity_scale_codec::{Decode, Encode};
use sc_executor::{WasmExecutionMethod, WasmtimeInstantiationStrategy};
use sc_executor_common::runtime_blob::RuntimeBlob;

const USAGE: &str = r#"
Usage: RUNTIME_WASM=<path> {}
"#;

fn main() -> Result<()> {
    color_eyre::install()?;

    let [_, runtime_wasm_path, out_path] = &env::args().collect::<Vec<_>>()[..] else {
        bail!("Usage: gsdk-scale-gen <runtime WASM path> <out .scale path>")
    };

    if env::args().len() < 1 {
        println!("{}", USAGE.trim());
        return Ok(());
    }

    // 1. Get the wasm binary of `RUNTIME_WASM`.
    let code = fs::read(runtime_wasm_path).context("Failed to read runtime wasm")?;

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

    // 4. Extract last supported metadata version
    let runtime_blob = RuntimeBlob::uncompress_if_needed(&code)?;
    let mut externalities = sp_io::TestExternalities::default();

    let versions = executor
        .uncached_call(
            runtime_blob.clone(),
            &mut externalities.ext(),
            true,
            "Metadata_metadata_versions",
            &[],
        )
        .unwrap();
    let versions = <Vec<u32>>::decode(&mut &versions[..])?;

    // This value is taken from `subxt`, which
    // unfortunately doesn't reexport it.
    let supported_versions = [14, 15, 16];
    let version = versions
        .into_iter()
        .filter(|version| supported_versions.contains(version))
        .max()
        .context("No supported metadata versions")?;

    // 3. Extract metadata.
    let option_bytes = executor.uncached_call(
        runtime_blob,
        &mut externalities.ext(),
        true,
        "Metadata_metadata_at_version",
        &version.encode(),
    )?;
    let bytes_option = <Option<Vec<u8>>>::decode(&mut &option_bytes[..])?;
    let metadata = bytes_option.context("Supported metadata format is not supported (how?)")?;

    fs::write(out_path, metadata)?;

    Ok(())
}
