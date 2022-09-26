//! environment paths and binaries
use lazy_static::lazy_static;

/// target path from the root workspace
const TARGET: &str = "target";
const WASM_TARGET: &str = "target/wasm32-unknown-unknown";

lazy_static! {
    static ref ROOT: String = env!("CARGO_MANIFEST_DIR").to_owned() + "/../";
}

fn bin_path(name: &str, wasm: bool) -> String {
    ROOT.clone()
        + [
            if wasm { WASM_TARGET } else { TARGET },
            "/",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            "/",
            name,
        ]
        .concat()
        .as_str()
}

/// path of binaries
pub fn bin(name: &str) -> String {
    bin_path(name, false)
}

/// path of wasm binaries
pub fn wasm_bin(name: &str) -> String {
    bin_path(name, true)
}
