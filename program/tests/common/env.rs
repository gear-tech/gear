//! environment paths and binaries
use lazy_static::lazy_static;
use std::path::PathBuf;

/// target path from the root workspace
const TARGET: &str = "target";

/// wasm target path from the root workspace
const WASM_TARGET: &str = "target/wasm32-unknown-unknown";

lazy_static! {
    static ref ROOT: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
}

/// path of binaries
pub fn bin(name: &str) -> PathBuf {
    ROOT.clone().join(
        &[
            TARGET,
            "/",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            "/",
            name,
        ]
        .concat(),
    )
}
