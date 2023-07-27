//! Build and import WASM libraries

use anyhow::Result;
use gear_wasm_builder::optimize::optimize_wasm;
use std::{collections::BTreeMap, env, fs, path::PathBuf, process::Command};

const LIBS_RS: &str = "libs.rs";
const LIBRARY_RELATIVE_DIR: &str = "../lib";
const LIBRARIES: [&str; 1] = ["dlmalloc"];
const OPT_LEVEL: &str = "z";

// Inject WASM libraries to `OUT_DIR/libs.rs`
//
// TODO: use `syn` and `proc-macro2` to generate code.
fn main() -> Result<()> {
    build()?;
    let mut libs_rs = String::new();

    // Inject WASM libraries.
    for (lib, path) in libs()? {
        let bytes = fs::read(path)?;

        libs_rs.push_str(&format!("/// GWASM library: {}\n", lib));
        // NOTE: consider using `hex`?
        libs_rs.push_str(&format!(
            "pub const {}: [u8; {}] = {:?};\n\n",
            lib.to_uppercase(),
            bytes.len(),
            bytes
        ));
    }

    // Inject helper iterator.
    {
        libs_rs.push_str("/// GWASM libraries\n");
        libs_rs.push_str(&format!(
            "pub const LIBS: [(&str, &[u8]); {}] = [",
            LIBRARIES.len()
        ));
        libs_rs.push_str(
            &LIBRARIES
                .iter()
                .map(|lib| format!("(\"{}\", &{}),", lib, lib.to_uppercase()))
                .collect::<String>(),
        );
        libs_rs.push_str("];\n");
    }

    fs::write(
        env::var("OUT_DIR").map(PathBuf::from)?.join(LIBS_RS),
        libs_rs,
    )?;

    Ok(())
}

// Build libraries.
fn build() -> Result<()> {
    Command::new("cargo")
        .args(&["build", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(LIBRARY_RELATIVE_DIR))
        .status()?;

    Ok(())
}

// Get built libraries.
//
// NOTE: ALWAYS use release builds.
fn libs() -> Result<BTreeMap<&'static str, PathBuf>> {
    let target = {
        let mut target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        target.push(format!(
            "{}/target/wasm32-unknown-unknown/release",
            LIBRARY_RELATIVE_DIR
        ));
        target.canonicalize()?
    };

    let libs = LIBRARIES
        .into_iter()
        .map(|lib| {
            let [src, dest] =
                ["wasm", "opt.wasm"].map(|ext| target.join(format!("gwasm_{lib}.{ext}")));

            optimize_wasm(src, dest.clone(), OPT_LEVEL, false)?;

            Ok((lib, dest))
        })
        .collect::<Result<_>>()?;

    Ok(libs)
}
