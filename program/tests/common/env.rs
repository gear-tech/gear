//! environment paths and binaries
use lazy_static::lazy_static;
use std::path::PathBuf;

/// target path from the root workspace
const TARGET: &str = "target";
const WASM_TARGET: &str = "target/wasm32-unknown-unknown";

lazy_static! {
    static ref ROOT: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
}

fn bin_path(name: &str, wasm: bool) -> PathBuf {
    ROOT.clone().join(
        &[
            if wasm { WASM_TARGET } else { TARGET },
            "/",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            "/",
            name,
            if wasm { ".opt.wasm" } else { "" },
        ]
        .concat(),
    )
}

/// path of binaries
pub fn bin(name: &str) -> PathBuf {
    bin_path(name, false)
}

/// path of wasm binaries
pub fn wasm_bin(name: &str) -> PathBuf {
    bin_path(name, true)
}
