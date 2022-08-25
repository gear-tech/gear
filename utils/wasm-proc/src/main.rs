// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#![allow(deprecated)]

use clap::Parser;
use gear_wasm_builder::optimize::{OptType, Optimizer};
use parity_wasm::elements::External;
use std::{collections::HashSet, fs, path::PathBuf};

const RT_ALLOWED_IMPORTS: [&str; 49] = [
    // From `Allocator` (substrate/primitives/io/src/lib.rs)
    "ext_allocator_free_version_1",
    "ext_allocator_malloc_version_1",
    // From `Crypto` (substrate/primitives/io/src/lib.rs)
    "ext_crypto_ed25519_generate_version_1",
    "ext_crypto_ed25519_verify_version_1",
    "ext_crypto_finish_batch_verify_version_1",
    "ext_crypto_secp256k1_ecdsa_recover_compressed_version_2",
    "ext_crypto_sr25519_generate_version_1",
    "ext_crypto_sr25519_verify_version_2",
    "ext_crypto_start_batch_verify_version_1",
    // From `GearRI` (runtime-interface/scr/lib.rs)
    "ext_gear_ri_get_released_pages_version_1",
    "ext_gear_ri_init_lazy_pages_version_2",
    "ext_gear_ri_initialize_for_program_version_1",
    "ext_gear_ri_is_lazy_pages_enabled_version_1",
    "ext_gear_ri_mprotect_lazy_pages_version_3",
    "ext_gear_ri_set_wasm_mem_begin_addr_version_2",
    "ext_gear_ri_set_wasm_mem_size_version_2",
    // From `Hashing` (substrate/primitives/io/src/lib.rs)
    "ext_hashing_blake2_128_version_1",
    "ext_hashing_blake2_256_version_1",
    "ext_hashing_twox_128_version_1",
    "ext_hashing_twox_64_version_1",
    // From `Logging` (substrate/primitives/io/src/lib.rs)
    "ext_logging_log_version_1",
    "ext_logging_max_level_version_1",
    // From `Misc` (substrate/primitives/io/src/lib.rs)
    "ext_misc_print_hex_version_1",
    "ext_misc_print_utf8_version_1",
    "ext_misc_runtime_version_version_1",
    // From `Sandbox` (substrate/primitives/io/src/lib.rs)
    "ext_sandbox_get_buff_version_1",
    "ext_sandbox_get_global_val_version_1",
    "ext_sandbox_instance_teardown_version_1",
    "ext_sandbox_instantiate_version_1",
    "ext_sandbox_invoke_version_1",
    "ext_sandbox_memory_get_version_1",
    "ext_sandbox_memory_grow_version_1",
    "ext_sandbox_memory_new_version_1",
    "ext_sandbox_memory_set_version_1",
    "ext_sandbox_memory_size_version_1",
    "ext_sandbox_memory_teardown_version_1",
    // From `Storage` (substrate/primitives/io/src/lib.rs)
    "ext_storage_append_version_1",
    "ext_storage_clear_prefix_version_2",
    "ext_storage_clear_version_1",
    "ext_storage_commit_transaction_version_1",
    "ext_storage_exists_version_1",
    "ext_storage_get_version_1",
    "ext_storage_next_key_version_1",
    "ext_storage_read_version_1",
    "ext_storage_rollback_transaction_version_1",
    "ext_storage_root_version_2",
    "ext_storage_set_version_1",
    "ext_storage_start_transaction_version_1",
    // From `Trie` (substrate/primitives/io/src/lib.rs)
    "ext_trie_blake2_256_ordered_root_version_2",
];

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Multiple skipping functional")]
    InvalidSkip,
}

#[derive(Debug, clap::Parser)]
struct Args {
    /// Path to WASMs, accepts multiple files
    #[clap(short, long, value_parser, multiple = true)]
    path: Vec<String>,

    /// Don't generate `.meta.wasm` file with meta functions
    #[clap(long)]
    skip_meta: bool,

    /// Don't generate `.opt.wasm` file
    #[clap(long)]
    skip_opt: bool,

    /// Don't export `__gear_stack_end`
    #[clap(long)]
    skip_stack_end: bool,

    /// Check runtime imports against the whitelist
    #[clap(long)]
    check_runtime_imports: bool,

    /// Verbose output
    #[clap(short, long)]
    verbose: bool,
}

fn check_rt_imports(path_to_wasm: &str, allowed_imports: &HashSet<&str>) -> Result<(), String> {
    let module = parity_wasm::deserialize_file(path_to_wasm)
        .map_err(|e| format!("Deserialization error: {e}"))?;
    let imports = module
        .import_section()
        .ok_or("Import section not found")?
        .entries();

    for import in imports {
        if matches!(import.external(), External::Function(_) if !allowed_imports.contains(import.field()))
        {
            return Err(format!("Unexpected import `{}`", import.field()));
        }
    }
    log::info!("{path_to_wasm} -> Ok");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        path: wasm_files,
        skip_meta,
        skip_opt,
        skip_stack_end,
        check_runtime_imports,
        verbose,
    } = Args::parse();

    let mut env = env_logger::Env::default();
    if verbose {
        env = env.default_filter_or("debug");
    }
    env_logger::Builder::from_env(env).init();

    if skip_meta && skip_opt {
        return Err(Box::new(Error::InvalidSkip));
    }

    let rt_allowed_imports: HashSet<&str> = RT_ALLOWED_IMPORTS.into();

    for file in &wasm_files {
        if !file.ends_with(".wasm") || file.ends_with(".meta.wasm") || file.ends_with(".opt.wasm") {
            continue;
        }

        if check_runtime_imports {
            check_rt_imports(file, &rt_allowed_imports)
                .map_err(|e| format!("Error with `{file}`: {e}"))?;
            continue;
        }

        let file = PathBuf::from(file);
        let res = gear_wasm_builder::optimize::optimize_wasm(file.clone(), "s", true)?;

        log::info!(
            "wasm-opt: {} {} Kb -> {} Kb",
            res.dest_wasm.display(),
            res.original_size,
            res.optimized_size
        );

        let mut optimizer = Optimizer::new(file.clone())?;

        if !skip_stack_end {
            optimizer.insert_stack_and_export();
        }

        if !skip_opt {
            let path = file.with_extension("opt.wasm");

            log::debug!("*** Processing chain optimization: {}", path.display());
            let code = optimizer.optimize(OptType::Opt)?;
            log::debug!("Optimized wasm: {}", path.to_string_lossy());

            fs::write(path, code)?;
        }

        if !skip_meta {
            let path = file.with_extension("meta.wasm");

            log::debug!("*** Processing metadata optimization: {}", path.display());
            let code = optimizer.optimize(OptType::Meta)?;
            log::debug!("Metadata wasm: {}", path.to_string_lossy());

            fs::write(path, code)?;
        }
    }

    Ok(())
}
