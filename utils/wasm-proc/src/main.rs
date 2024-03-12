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

use clap::Parser;
use gear_wasm_builder::{
    code_validator::CodeValidator,
    optimize::{self, OptType, Optimizer},
};
use parity_wasm::elements::External;
use std::{collections::HashSet, fs, path::PathBuf};

const RT_ALLOWED_IMPORTS: [&str; 67] = [
    // From `Allocator` (substrate/primitives/io/src/lib.rs)
    "ext_allocator_free_version_1",
    "ext_allocator_malloc_version_1",
    // From `Crypto` (substrate/primitives/io/src/lib.rs)
    "ext_crypto_ed25519_generate_version_1",
    "ext_crypto_ed25519_verify_version_1",
    "ext_crypto_finish_batch_verify_version_1",
    "ext_crypto_secp256k1_ecdsa_recover_compressed_version_2",
    "ext_crypto_sr25519_generate_version_1",
    "ext_crypto_sr25519_public_keys_version_1",
    "ext_crypto_sr25519_sign_version_1",
    "ext_crypto_sr25519_verify_version_2",
    "ext_crypto_start_batch_verify_version_1",
    // From `GearRI` (runtime-interface/scr/lib.rs)
    "ext_gear_ri_pre_process_memory_accesses_version_1",
    "ext_gear_ri_pre_process_memory_accesses_version_2",
    "ext_gear_ri_lazy_pages_status_version_1",
    "ext_gear_ri_write_accessed_pages_version_1",
    "ext_gear_ri_init_lazy_pages_version_1",
    "ext_gear_ri_init_lazy_pages_version_2",
    "ext_gear_ri_init_lazy_pages_for_program_version_1",
    "ext_gear_ri_is_lazy_pages_enabled_version_1",
    "ext_gear_ri_mprotect_lazy_pages_version_1",
    "ext_gear_ri_change_wasm_memory_addr_and_size_version_1",
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
    // From `OffchainIndex` (substrate/primitives/io/src/lib.rs)
    "ext_offchain_index_set_version_1",
    // From `Offchain` (substrate/primitives/io/src/lib.rs)
    "ext_offchain_is_validator_version_1",
    "ext_offchain_local_storage_compare_and_set_version_1",
    "ext_offchain_local_storage_get_version_1",
    "ext_offchain_local_storage_set_version_1",
    "ext_offchain_network_state_version_1",
    "ext_offchain_random_seed_version_1",
    "ext_offchain_submit_transaction_version_1",
    "ext_offchain_local_storage_clear_version_1",
    "ext_offchain_timestamp_version_1",
    // From `Sandbox` (substrate/primitives/io/src/lib.rs)
    "ext_sandbox_get_buff_version_1",
    "ext_sandbox_get_global_val_version_1",
    "ext_sandbox_set_global_val_version_1",
    "ext_sandbox_instance_teardown_version_1",
    "ext_sandbox_instantiate_version_1",
    "ext_sandbox_instantiate_version_2",
    "ext_sandbox_invoke_version_1",
    "ext_sandbox_memory_get_version_1",
    "ext_sandbox_memory_grow_version_1",
    "ext_sandbox_memory_new_version_1",
    "ext_sandbox_memory_set_version_1",
    "ext_sandbox_memory_size_version_1",
    "ext_sandbox_memory_teardown_version_1",
    "ext_sandbox_get_instance_ptr_version_1",
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

#[derive(Debug, clap::Parser)]
struct Args {
    /// Insert gear stack end export, enabled by default.
    #[arg(long, default_value = "true")]
    insert_stack_end: bool,

    #[arg(long)]
    assembly_script: bool,

    /// Strip custom sections of wasm binaries, enabled by default.
    #[arg(long, default_value = "true")]
    strip_custom_sections: bool,

    /// Check runtime imports against the whitelist
    #[arg(long)]
    check_runtime_imports: bool,

    /// Check runtime is built with dev feature or not
    #[arg(long)]
    check_runtime_is_dev: Option<bool>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Path to WASMs, accepts multiple files
    #[arg(value_parser)]
    path: Vec<String>,
}

fn check_rt_is_dev(path_to_wasm: &str, expected_to_be_dev: bool) -> Result<(), String> {
    let module = parity_wasm::deserialize_file(path_to_wasm)
        .map_err(|e| format!("Deserialization error: {e}"))?;

    let is_dev = module.custom_sections().any(|v| v.name() == "dev_runtime");

    match (expected_to_be_dev, is_dev) {
        (true, false) => Err(String::from("Runtime expected to be DEV, but it's NOT DEV")),
        (false, true) => Err(String::from("Runtime expected to be NOT DEV, but it's DEV")),
        _ => Ok(()),
    }
}

fn check_rt_imports(path_to_wasm: &str, allowed_imports: &HashSet<&str>) -> Result<(), String> {
    let module = parity_wasm::deserialize_file(path_to_wasm)
        .map_err(|e| format!("Deserialization error: {e}"))?;
    let imports = module
        .import_section()
        .ok_or("Import section not found")?
        .entries();

    let mut unexpected_imports = vec![];

    for import in imports {
        if matches!(import.external(), External::Function(_) if !allowed_imports.contains(import.field()))
        {
            unexpected_imports.push(import.field().to_string());
        }
    }

    if !unexpected_imports.is_empty() {
        return Err(format!(
            "Unexpected imports found: {}",
            unexpected_imports.join(", "),
        ));
    }

    log::info!("{path_to_wasm} -> Ok");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        path: wasm_files,
        mut insert_stack_end,
        assembly_script,
        strip_custom_sections,
        check_runtime_imports,
        check_runtime_is_dev,
        verbose,
    } = Args::parse();

    if assembly_script && insert_stack_end {
        log::debug!("Skip inserting stack end export, when as-script is enabled");
        insert_stack_end = false;
    }

    let mut env = env_logger::Env::default();
    if verbose {
        env = env.default_filter_or("debug");
    }
    env_logger::Builder::from_env(env).init();

    let rt_allowed_imports: HashSet<&str> = RT_ALLOWED_IMPORTS.into();

    for file in &wasm_files {
        if !file.ends_with(".wasm") || file.ends_with(".meta.wasm") || file.ends_with(".opt.wasm") {
            continue;
        }

        if check_runtime_imports {
            check_rt_imports(file, &rt_allowed_imports)
                .map_err(|e| format!("Error with `{file}`: {e}"))?;

            if check_runtime_is_dev.is_none() {
                continue;
            }
        }

        if let Some(expected_to_be_dev) = check_runtime_is_dev {
            check_rt_is_dev(file, expected_to_be_dev)
                .map_err(|e| format!("Error with `{file}`: {e}"))?;

            continue;
        }

        let original_wasm_path = PathBuf::from(file);
        let optimized_wasm_path = original_wasm_path.clone().with_extension("opt.wasm");

        // Make sure to run through gear semantic optimizer (preserving only required exports)
        let wasm_path = {
            let mut optimizer = Optimizer::new(original_wasm_path.clone())?;
            optimizer
                .insert_start_call_in_export_funcs()
                .expect("Failed to insert call _start in func exports");
            optimizer
                .move_mut_globals_to_static()
                .expect("Failed to move mutable globals to static");
            optimizer.flush_to_file(&optimized_wasm_path);
            optimized_wasm_path.clone()
        };

        // Make generic size optimizations by wasm-opt
        let res = optimize::optimize_wasm(wasm_path, optimized_wasm_path.clone(), "s", true)?;
        log::debug!(
            "wasm-opt has changed wasm size: {} Kb -> {} Kb",
            res.original_size,
            res.optimized_size
        );

        // Insert stack hint for optimized performance on-chain
        let mut optimizer = Optimizer::new(optimized_wasm_path.clone())?;
        if insert_stack_end {
            optimizer.insert_stack_end_export().unwrap_or_else(|err| {
                log::debug!("Failed to insert stack end: {}", err);
            })
        }

        // Make sure debug sections are stripped
        if strip_custom_sections {
            optimizer.strip_custom_sections();
        }

        log::debug!(
            "*** Processing chain optimization: {}",
            optimized_wasm_path.display()
        );
        let code = optimizer.optimize(OptType::Opt)?;
        log::info!("Optimized wasm: {}", optimized_wasm_path.to_string_lossy());

        fs::write(&optimized_wasm_path, &code)?;

        log::debug!(
            "*** Validating wasm code: {}",
            optimized_wasm_path.display()
        );

        CodeValidator::try_from(code)?.validate_program()?;
    }

    Ok(())
}
