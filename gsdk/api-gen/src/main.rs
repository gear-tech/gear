use color_eyre::eyre::Result;
use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::Decode;
use proc_macro2::TokenStream;
use std::{
    env, fs,
    io::{self, Write},
};
use subxt_codegen::{DerivesRegistry, RuntimeGenerator, TypeSubstitutes};

const RUNTIME_WASM: &'static str = "RUNTIME_WASM";
const USAGE: &'static str = r#"
Usage: RUNTIME_WASM=<path> generate-client-api
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
        // TODO: customized code here.
    }

    // Generate api.
    let runtime_types = generate_runtime_types(metadata).to_string();
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
    let code = fs::read(&path).expect("Failed to read runtime wasm");

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
    let runtime_types_mod = syn::parse_quote!(
        pub mod runtime_types {}
    );

    let crate_path = Default::default();
    generator
        .generate_runtime_types(
            runtime_types_mod,
            DerivesRegistry::new(&crate_path),
            TypeSubstitutes::new(&crate_path),
            crate_path,
            true,
        )
        .expect("Failed to generate runtime types")
}
