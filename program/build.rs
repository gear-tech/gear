//! build script for gear-program cli
#![allow(dead_code)]

use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::Decode;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use substrate_wasm_builder::WasmBuilder;
use subxt_codegen::DerivesRegistry;
use syn::ItemMod;

const WASM_MAGIC_NUMBER_PREFIX: usize = 4;

/// Runtime types.
pub enum Runtime {
    Gear,
    Vara,
}

impl Runtime {
    /// Converts the given runtime type to str.
    fn as_str(&self) -> &str {
        match self {
            Runtime::Gear => "gear",
            Runtime::Vara => "vara",
        }
    }

    // Using `gear-runtime` and `vara-runtime` as build denpendencies
    // will compile both native libraries and wasm libraries separately
    // from the complation of dependencies which is overkill for our need.
    //
    // So here we just build the wasm libraries from the runtimes diectly,
    // it will skip building the runtimes as native libraries and the builts
    // are shared with other workspace members.
    fn compile_runtime(&self, root: impl AsRef<Path>) {
        let runtime = root
            .as_ref()
            .join(format!("runtime/{}/Cargo.toml", self.as_str()));

        WasmBuilder::new()
            .with_project(&runtime)
            .expect(&format!("Failed to locate wasm project {:?}", runtime))
            .export_heap_base()
            .import_memory()
            .build()
    }

    // Get metadata from the wasm binaries of runtimes.
    pub fn metadata(&self) -> Vec<u8> {
        use gear_runtime_interface as gear_ri;
        use sc_executor::WasmExecutionMethod;
        use sc_executor_common::runtime_blob::RuntimeBlob;

        // 1. Get the runtime binary.
        let profile = env::var("PROFILE").expect("Unable to get build profile.");
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.parent().expect("Invalid manifest directory.");
        let path = root.join(format!(
            "target/{}/wbuild/{}-runtime/{}_runtime.compact.compressed.wasm",
            profile,
            self.as_str(),
            self.as_str(),
        ));

        // 2. Compile the runtime if it has not been compiled.
        if !path.exists() {
            self.compile_runtime(&root);
        }

        // 3. Create wasm executor.
        let executor = sc_executor::WasmExecutor::<(
            gear_ri::gear_ri::HostFunctions,
            sp_io::SubstrateHostFunctions,
        )>::new(WasmExecutionMethod::Interpreted, Some(1024), 8, None, 2);

        // 4. Extract metadata.
        let code = fs::read(&path).expect("Failed to find runtime");
        executor
            .uncached_call(
                RuntimeBlob::uncompress_if_needed(&code).expect("Invalid runtime code."),
                &mut sp_io::TestExternalities::default().ext(),
                true,
                "Metadata_metadata",
                &[],
            )
            .expect("Failed to extract runtime metadata")[WASM_MAGIC_NUMBER_PREFIX..]
            .to_vec()
    }
}

/// Generate API.
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

/// Write API to `OUT_DIR`.
fn write_api(api: &str, path: PathBuf) {
    // format generated code
    let mut rustfmt = Command::new("rustfmt");
    let mut code = rustfmt
        .args(["--edition=2021"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // Pipe API to rustfmt.
    write!(code.stdin.as_mut().unwrap(), "{api}")
        .expect("Failed to pipe generated code to rustfmt");
    let output = code.wait_with_output().expect("Broken pipe");

    // Write API to `OUT_DIR`.
    fs::write(&path, &output.stdout).expect(&format!("Couldn't write to file: {:?}", path));
}

/// Update runtime api.
fn update_api() {
    let path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not found")).join("metadata.rs");

    #[cfg(any(
        all(feature = "gear", not(feature = "vara")),
        all(feature = "gear", feature = "vara")
    ))]
    {
        write_api(
            &codegen(
                &Runtime::Gear.metadata(),
                syn::parse_quote!(
                    pub mod metadata {}
                ),
            ),
            path,
        );
    }

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    {
        write_api(
            &codegen(
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
