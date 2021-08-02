use clap::{App, Arg};
use pwasm_utils::{self as utils, parity_wasm};
use std::path::PathBuf;

fn main() {
    let matches = App::new("wasm-proc")
        .arg(
            Arg::with_name("input")
                .index(1)
                .required(true)
                .help("Input WASM file"),
        )
        .get_matches();

    let input = matches
        .value_of("input")
        .expect("Input paramter is required by clap above; qed");

    let module = parity_wasm::deserialize_file(&input).expect("Failed to load wasm file");

    // Invoke optimizer for the chain
    let mut binary_module = module.clone();
    let binary_file_name = PathBuf::from(input).with_extension("opt.wasm");
    utils::optimize(&mut binary_module, vec!["handle", "init"]).expect("Optimizer failed");
    parity_wasm::serialize_to_file(binary_file_name.clone(), binary_module)
        .expect("Serialization failed");

    println!("Optimized wasm: {}", binary_file_name.to_string_lossy());

    // Invoke optimizer for the metadata
    let mut metadata_module = module.clone();
    let metadata_file_name = PathBuf::from(input).with_extension("meta.wasm");
    utils::optimize(&mut metadata_module, vec!["meta_input", "meta_output"])
        .expect("Metadata optimizer failed");
    parity_wasm::serialize_to_file(metadata_file_name.clone(), metadata_module)
        .expect("Serialization failed");

    println!("Metadata wasm: {}", metadata_file_name.to_string_lossy());
}
