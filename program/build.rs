//! build script for gear-program cli
#![allow(dead_code)]

use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::Decode;
use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use subxt_codegen::DerivesRegistry;
use syn::ItemMod;

/// TODO: workaround for #2140, remove this after #2022
pub enum Runtime {
    Gear,
    Vara,
}

impl Runtime {
    // Get metadata from the wasm binaries of runtimes.
    pub fn metadata(&self) -> Vec<u8> {
        use gear_runtime_interface as gear_ri;
        use sc_executor::WasmExecutionMethod;
        use sc_executor_common::runtime_blob::RuntimeBlob;

        // 1. Get the runtime binary.
        let profile = env::var("PROFILE").unwrap();
        let runtime = match self {
            Runtime::Gear => "gear",
            Runtime::Vara => "vara",
        };
        let path = PathBuf::from(format!(
            "{}/../target/{}/wbuild/{}-runtime/{}_runtime.compact.compressed.wasm",
            env!("CARGO_MANIFEST_DIR"),
            profile,
            runtime,
            runtime
        ));

        // Prebuild runtime if it has not been compiled.
        if !path.exists() {
            let mut cargo = Command::new("cargo");
            let pkg = runtime.to_owned() + "-runtime";
            let mut args = vec!["b", "-p", &pkg];
            if profile == "release" {
                args.push("--release");
            }
            cargo.args(&args).status().expect("Format code failed.");
        }

        let code = fs::read(&path).expect("Failed to find runtime");

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
            .expect("Failed to extract runtime metadata")[4..] // [4..] for removing the magic number.
            .to_vec()
    }
}

/// Generate api
fn codegen(mut encoded: &[u8], item_mod: ItemMod) -> String {
    let metadata =
        <RuntimeMetadataPrefixed as Decode>::decode(&mut encoded).expect("decode metadata failed");

    // Genreate code.
    let crate_path = Default::default();
    let generator = subxt_codegen::RuntimeGenerator::new(metadata);
    generator
        .generate_runtime(item_mod, DerivesRegistry::new(&crate_path), crate_path)
        .to_string()
}

/// Write API to disk
fn write_api(api: &str, path: PathBuf) {
    // format generated code
    let mut rustfmt = Command::new("rustfmt");
    let mut code = rustfmt
        .args(["--edition=2021"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // pipe api to rustfmt
    write!(code.stdin.as_mut().unwrap(), "{api}").unwrap();
    let output = code.wait_with_output().unwrap();

    // write api to disk
    fs::write(&path, &output.stdout).expect(&format!("Couldn't write to file: {:?}", path));
}

/// Update runtime api
fn update_api() {
    let path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not found")).join("metadata.rs");

    // Clean previous generation if exists.
    let _ = fs::remove_file(&path);

    #[cfg(any(
        all(feature = "gear", not(feature = "vara")),
        all(feature = "gear", feature = "vara")
    ))]
    {
        write_api(
            &codegen(
                // &gear_runtime::Runtime::metadata().encode(),
                //
                // TODO: remove the following line after #2022.
                &Runtime::Gear.metadata(),
                syn::parse_quote!(
                    pub mod metadata {}
                ),
            ),
            path.clone(),
        );
    }

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    {
        write_api(
            &codegen(
                // &vara_runtime::Runtime::metadata().encode(),
                //
                // TODO: remove the following line after #2022.
                &Runtime::Vara.metadata(),
                syn::parse_quote!(
                    pub mod metadata {}
                ),
            ),
            path,
        );
    }

    // # NOTE
    //
    // post format code since `cargo +nightly fmt` doesn't support pipe
    let mut cargo = Command::new("cargo");
    cargo
        .args(["+nightly", "fmt"])
        .status()
        .expect("Format code failed.");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=../runtime");
    println!("cargo:rerun-if-changed=../pallets/gear");

    update_api();
}
