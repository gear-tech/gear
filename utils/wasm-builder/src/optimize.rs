use crate::{builder_error::BuilderError, stack_end};
use anyhow::{Context, Result};
#[cfg(not(feature = "wasm-opt"))]
use colored::Colorize;
use gear_core::code::Code;
use gear_wasm_instrument::{rules::CustomConstantCostRules, STACK_END_EXPORT_NAME};
use pwasm_utils::{
    parity_wasm,
    parity_wasm::elements::{Internal, Module, Section, Serialize},
};
#[cfg(not(feature = "wasm-opt"))]
use std::process::Command;
use std::{
    ffi::OsStr,
    fs::{self, metadata},
    path::PathBuf,
};

#[cfg(feature = "wasm-opt")]
use wasm_opt::{OptimizationOptions, Pass};

pub const FUNC_EXPORTS: [&str; 4] = ["init", "handle", "handle_reply", "handle_signal"];

const OPTIMIZED_EXPORTS: [&str; 7] = [
    "handle",
    "handle_reply",
    "handle_signal",
    "init",
    "state",
    "metahash",
    STACK_END_EXPORT_NAME,
];

/// Type of the output wasm.
#[derive(PartialEq, Eq)]
pub enum OptType {
    Meta,
    Opt,
}

#[derive(Debug, thiserror::Error)]
#[error("Optimizer failed: {0:?}")]
pub struct OptimizerError(pwasm_utils::OptimizerError);

pub struct Optimizer {
    module: Module,
    file: PathBuf,
}

impl Optimizer {
    pub fn new(file: PathBuf) -> Result<Self> {
        let contents = fs::read(&file)?;
        let module = parity_wasm::deserialize_buffer(&contents)
            .with_context(|| format!("File path: {file:?}"))?;
        Ok(Self { module, file })
    }

    pub fn insert_start_call_in_export_funcs(&mut self) -> Result<(), &'static str> {
        stack_end::insert_start_call_in_export_funcs(&mut self.module)
    }

    pub fn move_mut_globals_to_static(&mut self) -> Result<(), &'static str> {
        stack_end::move_mut_globals_to_static(&mut self.module)
    }

    pub fn insert_stack_end_export(&mut self) -> Result<(), &'static str> {
        stack_end::insert_stack_end_export(&mut self.module)
    }

    /// Strips all custom sections.
    ///
    /// Presently all custom sections are not required so they can be stripped safely.
    /// The name section is already stripped by `wasm-opt`.
    pub fn strip_custom_sections(&mut self) {
        self.module
            .sections_mut()
            .retain(|section| !matches!(section, Section::Reloc(_) | Section::Custom(_)))
    }

    pub fn flush_to_file(self, path: &PathBuf) {
        fs::write(path, self.module.to_bytes().unwrap()).unwrap();
    }

    /// Process optimization.
    pub fn optimize(&self, ty: OptType) -> Result<Vec<u8>> {
        let mut module = self.module.clone();

        let exports = if ty == OptType::Opt {
            OPTIMIZED_EXPORTS.to_vec()
        } else {
            self.module
                .export_section()
                .ok_or_else(|| anyhow::anyhow!("Export section not found"))?
                .entries()
                .iter()
                .flat_map(|entry| {
                    if let Internal::Function(_) = entry.internal() {
                        let entry = entry.field();
                        (!OPTIMIZED_EXPORTS.contains(&entry)).then_some(entry)
                    } else {
                        None
                    }
                })
                .collect()
        };

        pwasm_utils::optimize(&mut module, exports)
            .map_err(OptimizerError)
            .with_context(|| {
                format!(
                    "unable to optimize the WASM file `{0}`",
                    self.file.display()
                )
            })?;

        let mut code = vec![];
        module.serialize(&mut code)?;

        // Post-checking the program code for possible errors
        // `pallet-gear` crate performs the same check at the node level when the user tries to upload program code
        let raw_code = code.clone();
        match ty {
            // validate metawasm code
            // see `pallet_gear::pallet::Pallet::read_state_using_wasm(...)`
            OptType::Meta => {
                Code::new_raw_with_rules(raw_code, 1, false, |_| CustomConstantCostRules::default())
                    .map(|_| ())
                    .map_err(BuilderError::CodeCheckFailed)?
            }
            // validate wasm code
            // see `pallet_gear::pallet::Pallet::upload_program(...)`
            OptType::Opt => {
                Code::try_new(raw_code, 1, |_| CustomConstantCostRules::default(), None)
                    .map(|_| ())
                    .map_err(BuilderError::CodeCheckFailed)?
            }
        }

        Ok(code)
    }
}

pub struct OptimizationResult {
    pub original_size: f64,
    pub optimized_size: f64,
}

/// Attempts to perform optional Wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated Wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
pub fn optimize_wasm(
    source: PathBuf,
    destination: PathBuf,
    optimization_passes: &str,
    keep_debug_symbols: bool,
) -> Result<OptimizationResult> {
    let original_size = metadata(&source)?.len() as f64 / 1000.0;

    do_optimization(
        source.as_os_str(),
        destination.as_os_str(),
        optimization_passes,
        keep_debug_symbols,
    )?;

    if !destination.exists() {
        return Err(anyhow::anyhow!(
            "Optimization failed, optimized wasm output file `{}` not found.",
            destination.display()
        ));
    }

    let optimized_size = metadata(&destination)?.len() as f64 / 1000.0;

    Ok(OptimizationResult {
        original_size,
        optimized_size,
    })
}

#[cfg(not(feature = "wasm-opt"))]
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
        .arg(format!("-O{optimization_level}"))
        .arg("-o")
        .arg(dest_optimized)
        .arg("-mvp")
        .arg("--enable-sign-ext")
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
            The error which wasm-opt returned was: \n{err}"
        );
    }
    Ok(())
}

#[cfg(feature = "wasm-opt")]
/// Optimizes the Wasm supplied as `crate_metadata.dest_wasm` using
/// `wasm-opt`.
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
    log::info!(
        "Optimization level passed to wasm-opt: {}",
        optimization_level
    );
    match optimization_level {
        "0" => OptimizationOptions::new_opt_level_0(),
        "1" => OptimizationOptions::new_opt_level_1(),
        "2" => OptimizationOptions::new_opt_level_2(),
        "3" => OptimizationOptions::new_opt_level_3(),
        "4" => OptimizationOptions::new_opt_level_4(),
        "s" => OptimizationOptions::new_optimize_for_size(),
        "z" => OptimizationOptions::new_optimize_for_size_aggressively(),
        _ => panic!("Invalid optimization level {}", optimization_level),
    }
    .mvp_features_only()
    .enable_feature(wasm_opt::Feature::SignExt)
    .shrink_level(wasm_opt::ShrinkLevel::Level2)
    .add_pass(Pass::Dae)
    .add_pass(Pass::Vacuum)
    // the memory in our module is imported, `wasm-opt` needs to be told that
    // the memory is initialized to zeroes, otherwise it won't run the
    // memory-packing pre-pass.
    .zero_filled_memory(true)
    .debug_info(keep_debug_symbols)
    .run(dest_wasm, dest_optimized)?;

    Ok(())
}
