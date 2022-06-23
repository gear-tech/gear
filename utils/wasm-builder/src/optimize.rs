use crate::builder_error::BuilderError;
use anyhow::Result;
use colored::Colorize;
use pwasm_utils::parity_wasm::elements::{Internal, Module};
use std::{
    ffi::OsStr,
    fs::metadata,
    path::{Path, PathBuf},
    process::Command,
};

pub struct OptimizationResult {
    pub dest_wasm: PathBuf,
    pub original_size: f64,
    pub optimized_size: f64,
}

/// Attempts to perform optional Wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated Wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
pub fn optimize_wasm(
    source: PathBuf,
    optimization_passes: &str,
    keep_debug_symbols: bool,
) -> Result<OptimizationResult> {
    let mut dest_optimized = source.clone();

    dest_optimized.set_file_name(format!(
        "{}-opt.wasm",
        source
            .file_name()
            .unwrap_or_else(|| OsStr::new("program"))
            .to_str()
            .unwrap()
    ));

    do_optimization(
        source.as_os_str(),
        dest_optimized.as_os_str(),
        optimization_passes,
        keep_debug_symbols,
    )?;

    if !dest_optimized.exists() {
        return Err(anyhow::anyhow!(
            "Optimization failed, optimized wasm output file `{}` not found.",
            dest_optimized.display()
        ));
    }

    let original_size = metadata(&source)?.len() as f64 / 1000.0;
    let optimized_size = metadata(&dest_optimized)?.len() as f64 / 1000.0;

    // overwrite existing destination wasm file with the optimised version
    std::fs::rename(&dest_optimized, &source)?;
    Ok(OptimizationResult {
        dest_wasm: source,
        original_size,
        optimized_size,
    })
}

/// Optimizes the Wasm supplied as `crate_metadata.dest_wasm` using
/// the `wasm-opt` binary.
///
/// The supplied `optimization_level` denotes the number of optimization passes,
/// resulting in potentially a lot of time spent optimizing.
///
/// If successful, the optimized Wasm is written to `dest_optimized`.
pub fn do_optimization(
    dest_wasm: &OsStr,
    dest_optimized: &OsStr,
    optimization_level: &str,
    keep_debug_symbols: bool,
) -> Result<()> {
    // check `wasm-opt` is installed
    let which = which::which("wasm-opt");
    if which.is_err() {
        return Err(anyhow::anyhow!(
            "wasm-opt not found! Make sure the binary is in your PATH environment.\n\n\
            We use this tool to optimize the size of your contract's Wasm binary.\n\n\
            wasm-opt is part of the binaryen package. You can find detailed\n\
            installation instructions on https://github.com/WebAssembly/binaryen#tools.\n\n\
            There are ready-to-install packages for many platforms:\n\
            * Debian/Ubuntu: apt-get install binaryen\n\
            * Homebrew: brew install binaryen\n\
            * Arch Linux: pacman -S binaryen\n\
            * Windows: binary releases at https://github.com/WebAssembly/binaryen/releases"
                .bright_yellow()
        ));
    }
    let wasm_opt_path = which
        .as_ref()
        .expect("we just checked if `which` returned an err; qed")
        .as_path();
    log::info!("Path to wasm-opt executable: {}", wasm_opt_path.display());

    log::info!(
        "Optimization level passed to wasm-opt: {}",
        optimization_level
    );
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(dest_wasm)
        .arg(format!("-O{}", optimization_level))
        .arg("-o")
        .arg(dest_optimized)
        // the memory in our module is imported, `wasm-opt` needs to be told that
        // the memory is initialized to zeroes, otherwise it won't run the
        // memory-packing pre-pass.
        .arg("--zero-filled-memory")
        .arg("--dae")
        .arg("--vacuum");
    if keep_debug_symbols {
        command.arg("-g");
    }
    log::info!("Invoking wasm-opt with {:?}", command);
    let output = command.output().unwrap();

    if !output.status.success() {
        let err = std::str::from_utf8(&output.stderr)
            .expect("Cannot convert stderr output of wasm-opt to string")
            .trim();
        panic!(
            "The wasm-opt optimization failed.\n\n\
            The error which wasm-opt returned was: \n{}",
            err
        );
    }
    Ok(())
}

pub fn check_exports(module: &Module, path: &Path) -> Result<()> {
    if module
        .export_section()
        .ok_or_else(|| BuilderError::ExportSectionNotFound(path.to_path_buf()))?
        .entries()
        .iter()
        .any(|entry| {
            matches!(entry.internal(), Internal::Function(_))
                && matches!(entry.field(), "init" | "handle")
        })
    {
        Ok(())
    } else {
        Err(BuilderError::RequiredExportFnNotFound(path.to_path_buf()).into())
    }
}
