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

#[cfg(feature = "std")]
fn skip_build_on_intellij_sync() {
    // Intellij Rust uses rustc wrapper during project sync
    let is_intellij = std::env::var("RUSTC_WRAPPER")
        .unwrap_or_default()
        .contains("intellij");
    if is_intellij {
        unsafe { std::env::set_var("SKIP_WASM_BUILD", "1") }
    }
}

#[cfg(all(feature = "std", not(feature = "metadata-hash")))]
fn main() {
    substrate_build_script_utils::generate_cargo_keys();
    #[cfg(all(feature = "std", not(fuzz)))]
    {
        skip_build_on_intellij_sync();
        substrate_wasm_builder::WasmBuilder::build_using_defaults();
        regenerate_gsdk_scale();
    }
}

#[cfg(all(feature = "std", feature = "metadata-hash"))]
fn main() {
    substrate_build_script_utils::generate_cargo_keys();
    #[cfg(all(feature = "std", not(fuzz)))]
    {
        const TOKEN_SYMBOL: &str = if cfg!(not(feature = "dev")) {
            "VARA"
        } else {
            "TVARA"
        };

        const DECIMALS: u8 = 12;

        skip_build_on_intellij_sync();

        substrate_wasm_builder::WasmBuilder::init_with_defaults()
            .enable_metadata_hash(TOKEN_SYMBOL, DECIMALS)
            .build();
        regenerate_gsdk_scale();
    }
}

#[cfg(not(feature = "std"))]
fn main() {
    substrate_build_script_utils::generate_cargo_keys();
}

#[cfg(feature = "std")]
fn regenerate_gsdk_scale() {
    use gear_runtime_interface::gear_ri;
    use parity_scale_codec::{Decode, Encode};
    use sc_executor::{WasmExecutionMethod, WasmtimeInstantiationStrategy};
    use sc_executor_common::runtime_blob::RuntimeBlob;
    use std::{env, fs, path::PathBuf};

    let out_path = "../../gsdk/vara_runtime.scale";
    
    #[cfg(not(feature = "dev"))]
    let out_path = "../../gsdk/vara_runtime_prod.scale";

    let runtime_wasm_path = PathBuf::from(env::var("OUT_DIR").unwrap())
        .ancestors()
        .find(|dir| {
            dir.file_name()
                .is_some_and(|name| name.to_str() == Some("build"))
        })
        .unwrap()
        .parent()
        .unwrap()
        .join("wbuild/vara-runtime/vara_runtime.wasm");

    if env::var("SKIP_WASM_BUILD").is_ok() || env::var("SKIP_VARA_RUNTIME_WASM_BUILD").is_ok() {
        return;
    }

    // 1. Get the wasm binary of `RUNTIME_WASM`.
    let code = fs::read(runtime_wasm_path).expect("Failed to read runtime wasm");

    let heap_pages =
        sc_executor_common::wasm_runtime::HeapAllocStrategy::Static { extra_pages: 1024 };

    // 2. Create wasm executor.
    let executor = sc_executor::WasmExecutor::<(
        gear_ri::HostFunctions,
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
    let runtime_blob = RuntimeBlob::uncompress_if_needed(&code).unwrap();
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
    let versions = <Vec<u32>>::decode(&mut &versions[..]).unwrap();

    // List of metadata versions supported by `frame-metadata` and `subxt`
    let supported_versions = [14, 15, 16];
    let version = versions
        .into_iter()
        .filter(|version| supported_versions.contains(version))
        .max()
        .expect("No supported metadata versions");

    // 3. Extract metadata.
    let option_bytes = executor
        .uncached_call(
            runtime_blob,
            &mut externalities.ext(),
            true,
            "Metadata_metadata_at_version",
            &version.encode(),
        )
        .unwrap();
    let bytes_option = <Option<Vec<u8>>>::decode(&mut &option_bytes[..]).unwrap();
    let metadata = bytes_option.expect("Supported metadata format is not supported (how?)");

    fs::write(out_path, metadata).unwrap();
}
