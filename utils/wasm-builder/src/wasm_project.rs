// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{code_validator::validate_program, crate_info::CrateInfo, smart_fs};
use anyhow::{anyhow, Context, Ok, Result};
use chrono::offset::Local as ChronoLocal;
use gear_wasm_optimizer::{self as optimize, Optimizer};
use itertools::Itertools;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use toml::value::Table;

const OPT_LEVEL: &str = "z";

/// Temporary project generated to build a WASM output.
///
/// This project is required due to the cargo locking during build.
pub struct WasmProject {
    original_dir: PathBuf,
    out_dir: PathBuf,
    target_dir: PathBuf,
    wasm_target_dir: PathBuf,
    file_base_name: Option<String>,
    profile: String,
    features: Option<Vec<String>>,
    pre_processors: Vec<Box<dyn PreProcessor>>,
}

/// Pre-processor result.
pub type PreProcessorResult<T> = Result<T>;

/// Preprocessor target specifying the output file name.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum PreProcessorTarget {
    /// Use the default file name (i.e. overwrite the .wasm file).
    #[default]
    Default,
    /// Use the given file name with pre-processor name at end.
    ///
    /// `Named("foo.wasm".into())` will be converted to
    /// `"foo_{pre_processor_name}.wasm"`.
    Named(String),
}

/// Pre-processor hook for wasm generation.
pub trait PreProcessor {
    /// Returns the name of the pre-processor. It must be some unique string.
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    /// Returns the result of the pre-processor.
    fn pre_process(
        &self,
        original: PathBuf,
    ) -> PreProcessorResult<Vec<(PreProcessorTarget, Vec<u8>)>>;
}

impl WasmProject {
    /// Create a new `WasmProject`.
    pub fn new() -> Self {
        let original_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")
            .expect("`CARGO_MANIFEST_DIR` is always set in build scripts")
            .into();

        let out_dir: PathBuf = env::var("OUT_DIR")
            .expect("`OUT_DIR` is always set in build scripts")
            .into();

        let substrate_runtime = env::var("CARGO_CFG_SUBSTRATE_RUNTIME").is_ok();
        let mut first_target_reached = false;

        let profile = out_dir
            .components()
            .rev()
            .take_while(|c| {
                if c.as_os_str() != "target" {
                    true
                } else if substrate_runtime && !first_target_reached {
                    first_target_reached = true;
                    true
                } else {
                    false
                }
            })
            .collect::<PathBuf>()
            .components()
            .rev()
            .take_while(|c| c.as_os_str() != "build" && c.as_os_str() != "wbuild")
            .last()
            .expect("Path should have subdirs in the `target` dir")
            .as_os_str()
            .to_string_lossy()
            .into();

        let mut target_dir = out_dir
            .ancestors()
            .find(|path| path.ends_with(&profile) && !path.iter().contains(&OsStr::new("wbuild")))
            .and_then(|path| path.parent())
            .expect("Could not find target directory")
            .to_owned();

        let mut wasm_target_dir = target_dir.clone();

        // remove component to avoid creating a directory inside
        // `target/x86_64-unknown-linux-gnu` and so on when cross-compiling
        if env::var("HOST") != env::var("TARGET") {
            wasm_target_dir.pop();
        }

        wasm_target_dir.push("wasm32-gear");
        wasm_target_dir.push(&profile);

        target_dir.push("wasm-projects");
        target_dir.push(&profile);

        WasmProject {
            original_dir,
            out_dir,
            target_dir,
            wasm_target_dir,
            file_base_name: None,
            profile,
            features: None,
            pre_processors: vec![],
        }
    }

    /// Add pre-processor for wasm file.
    pub fn add_preprocessor(&mut self, pre_processor: Box<dyn PreProcessor>) {
        self.pre_processors.push(pre_processor)
    }

    /// Return the path to the temporary generated `Cargo.toml`.
    pub fn manifest_path(&self) -> PathBuf {
        self.out_dir.join("Cargo.toml")
    }

    /// Return the path to the target directory.
    pub fn target_dir(&self) -> PathBuf {
        self.target_dir.clone()
    }

    /// Return the profile name based on the `OUT_DIR` path.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Return features available in the generated `Cargo.toml`.
    pub fn features(&self) -> &[String] {
        self.features
            .as_ref()
            .expect("Run `WasmProject::generate()` first")
    }

    /// Generate a temporary cargo project that includes the original package as
    /// a dependency.
    pub fn generate(&mut self) -> Result<()> {
        let original_manifest = self.original_dir.join("Cargo.toml");
        let crate_info = CrateInfo::from_manifest(&original_manifest)?;
        self.file_base_name = Some(crate_info.snake_case_name.clone());

        let mut package = Table::new();
        package.insert("name".into(), format!("{}-wasm", &crate_info.name).into());
        package.insert("version".into(), crate_info.version.into());
        package.insert("edition".into(), "2024".into());

        let mut lib = Table::new();
        lib.insert("name".into(), crate_info.snake_case_name.into());
        lib.insert("crate-type".into(), vec!["cdylib".to_string()].into());

        let mut dev_profile = Table::new();
        dev_profile.insert("opt-level".into(), OPT_LEVEL.into());

        let mut release_profile = Table::new();
        release_profile.insert("lto".into(), "fat".into());
        release_profile.insert("opt-level".into(), OPT_LEVEL.into());
        release_profile.insert("codegen-units".into(), 1.into());

        let mut production_profile = Table::new();
        production_profile.insert("inherits".into(), "release".into());

        let mut profile = crate_info.profiles;
        profile.insert("dev".into(), dev_profile.clone().into());
        profile.insert("release".into(), release_profile.into());
        profile.insert("production".into(), production_profile.into());

        let mut crate_package = Table::new();
        crate_package.insert("package".into(), crate_info.name.into());
        crate_package.insert(
            "path".into(),
            self.original_dir.display().to_string().into(),
        );
        crate_package.insert("default-features".into(), false.into());

        let mut dependencies = Table::new();
        dependencies.insert("orig-project".into(), crate_package.into());

        let mut features = Table::new();
        for feature in crate_info.features.keys() {
            if feature != "default" {
                features.insert(
                    feature.into(),
                    vec![format!("orig-project/{feature}")].into(),
                );
            }
        }
        self.features = Some(features.keys().cloned().collect());

        let mut cargo_toml = Table::new();
        cargo_toml.insert("package".into(), package.into());
        cargo_toml.insert("lib".into(), lib.into());
        cargo_toml.insert("dependencies".into(), dependencies.into());
        cargo_toml.insert("profile".into(), profile.into());
        cargo_toml.insert("features".into(), features.into());
        cargo_toml.insert("workspace".into(), Table::new().into());
        cargo_toml.insert("patch".into(), crate_info.patch.into());

        smart_fs::write(self.manifest_path(), toml::to_string_pretty(&cargo_toml)?)
            .context("Failed to write generated manifest path")?;

        // Copy original `Cargo.lock` if any
        let from_lock = self.original_dir.join("Cargo.lock");
        let to_lock = self.out_dir.join("Cargo.lock");
        drop(fs::copy(from_lock, to_lock));

        let source_code =
            "#![no_std]\n#[allow(unused_imports)]\npub use orig_project::*;\n".to_owned();

        fs::create_dir_all(&self.wasm_target_dir).with_context(|| {
            format!(
                "Failed to create WASM target directory: {}",
                self.wasm_target_dir.display()
            )
        })?;

        let src_dir = self.out_dir.join("src");
        fs::create_dir_all(&src_dir).context("Failed to create `src` directory")?;
        smart_fs::write(src_dir.join("lib.rs"), source_code)?;

        Ok(())
    }

    fn generate_bin_path(&self, file_base_name: &str) -> Result<()> {
        let relative_path_to_wasm = pathdiff::diff_paths(&self.wasm_target_dir, &self.original_dir)
            .with_context(|| {
                format!(
                    "wasm_target_dir={}; original_dir={}",
                    self.wasm_target_dir.display(),
                    self.original_dir.display()
                )
            })
            .expect("Unable to calculate relative path")
            .join(file_base_name);

        smart_fs::write(
            self.original_dir.join(".binpath"),
            format!("{}", relative_path_to_wasm.display()),
        )
        .context("unable to write `.binpath`")?;

        Ok(())
    }

    pub fn file_base_name(&self) -> &str {
        self.file_base_name
            .as_ref()
            .expect("Run `WasmProject::generate()` first")
    }

    pub fn wasm_paths(&self, file_base_name: &str) -> (PathBuf, PathBuf) {
        let [original_wasm_path, opt_wasm_path] = [".wasm", ".opt.wasm"]
            .map(|ext| self.wasm_target_dir.join([file_base_name, ext].concat()));
        (original_wasm_path, opt_wasm_path)
    }

    /// Generates output optimized wasm file, `.binpath` file for our tests
    /// system and wasm binaries informational file.
    /// Makes a copy of original wasm file in `self.wasm_target_dir`.
    pub fn postprocess_opt<P: AsRef<Path>>(
        &self,
        original_wasm_path: P,
        file_base_name: &str,
    ) -> Result<PathBuf> {
        let (original_copy_wasm_path, opt_wasm_path) = self.wasm_paths(file_base_name);

        // Copy original file to `self.wasm_target_dir`
        smart_fs::copy_if_newer(&original_wasm_path, &original_copy_wasm_path).with_context(
            || {
                format!(
                    "unable to copy WASM file from {}",
                    original_copy_wasm_path.display()
                )
            },
        )?;

        // Optimize wasm using and `wasm-opt` and our optimizations.
        if smart_fs::check_if_newer(&original_wasm_path, &opt_wasm_path)? {
            let mut optimizer = Optimizer::new(&original_copy_wasm_path)?;
            optimizer
                .insert_stack_end_export()
                .unwrap_or_else(|err| log::info!("Cannot insert stack end export: {err}"));
            optimizer.strip_custom_sections();
            optimizer.strip_exports();
            optimizer.flush_to_file(&opt_wasm_path);

            optimize::optimize_wasm(&opt_wasm_path, &opt_wasm_path, "4", true)
                .map(|res| {
                    log::info!(
                        "Wasm-opt reduced wasm size: {} -> {}",
                        res.original_size,
                        res.optimized_size
                    );
                })
                .unwrap_or_else(|err| {
                    println!("cargo:warning=wasm-opt optimizations error: {err}");
                });
        }

        // Create `wasm_binary.rs`
        smart_fs::write(
            self.out_dir.join("wasm_binary.rs"),
            format!(
                r#"#[allow(unused)]
pub const WASM_BINARY: &[u8] = include_bytes!("{}");
#[allow(unused)]
pub const WASM_BINARY_OPT: &[u8] = include_bytes!("{}");"#,
                display_path(original_copy_wasm_path.as_path()),
                display_path(opt_wasm_path.as_path()),
            ),
        )
        .context("unable to write `wasm_binary.rs`")?;
        Ok(opt_wasm_path)
    }

    /// Post-processing after the WASM binary has been built.
    ///
    /// - Copy WASM binary from `OUT_DIR` to
    ///   `target/wasm32-gear/<profile>`
    /// - Generate optimized binary from the built program
    /// - Generate `wasm_binary.rs` source file in `OUT_DIR`
    pub fn postprocess(&self) -> Result<Option<(PathBuf, PathBuf)>> {
        let file_base_name = self.file_base_name();

        let original_wasm_path = self.target_dir.join(format!(
            "wasm32v1-none/{}/{file_base_name}.wasm",
            self.profile
        ));

        fs::create_dir_all(&self.target_dir).context("Failed to create target directory")?;

        self.generate_bin_path(file_base_name)?;

        let mut wasm_files = vec![(original_wasm_path.clone(), file_base_name.to_string())];

        for pre_processor in &self.pre_processors {
            let pre_processor_name = pre_processor.name().to_lowercase().replace('-', "_");
            let pre_processor_output = pre_processor.pre_process(original_wasm_path.clone())?;

            let default_targets = pre_processor_output
                .iter()
                .filter(|(target, _)| *target == PreProcessorTarget::Default)
                .count();
            if default_targets > 1 {
                return Err(anyhow!(
                    "Pre-processor \"{pre_processor_name}\" cannot have more than one default target."
                ));
            }

            for (pre_processor_target, content) in pre_processor_output {
                let (pre_processed_path, new_wasm_file) = match pre_processor_target {
                    PreProcessorTarget::Default => (original_wasm_path.clone(), false),
                    PreProcessorTarget::Named(filename) => {
                        let path = Path::new(&filename);

                        let file_stem = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .expect("Failed to get file stem");
                        let extension = path.extension().and_then(|s| s.to_str());

                        let filename = match extension {
                            Some(extension) => {
                                format!("{file_stem}_{pre_processor_name}.{extension}")
                            }
                            None => format!("{file_stem}_{pre_processor_name}"),
                        };

                        (
                            self.wasm_target_dir.join(filename),
                            extension == Some("wasm"),
                        )
                    }
                };
                fs::write(&pre_processed_path, content)?;

                if new_wasm_file {
                    let file_stem = pre_processed_path
                        .file_stem()
                        .and_then(|s| s.to_str().map(|s| s.to_string()))
                        .expect("Failed to get file stem");
                    wasm_files.push((pre_processed_path, file_stem));
                }
            }
        }

        // Tuple with PathBuf last wasm & opt.wasm
        let mut wasm_paths: Option<(PathBuf, PathBuf)> = None;
        for (wasm_path, file_base_name) in &wasm_files {
            let wasm_opt = self.postprocess_opt(wasm_path, file_base_name)?;
            wasm_paths = Some((wasm_path.clone(), wasm_opt));
        }

        for (wasm_path, _) in &wasm_files {
            let code = fs::read(wasm_path)?;

            validate_program(code)?;
        }

        if env::var("__GEAR_WASM_BUILDER_NO_FEATURES_TRACKING").is_err() {
            self.force_rerun_on_next_run(&original_wasm_path)?;
        }
        Ok(wasm_paths)
    }

    // Force cargo to re-run the build script next time it is invoked.
    // It is needed because feature set or toolchain can change.
    fn force_rerun_on_next_run(&self, wasm_file_path: &Path) -> Result<()> {
        let stamp_file_path = wasm_file_path.with_extension("stamp");
        fs::write(&stamp_file_path, ChronoLocal::now().to_rfc3339())
            .context("Failed to write stamp file")?;
        println!("cargo:rerun-if-changed={}", stamp_file_path.display());
        Ok(())
    }

    /// Provide a dummy WASM binary if there doesn't exist one.
    pub fn provide_dummy_wasm_binary_if_not_exist(&self) -> PathBuf {
        let wasm_binary_rs = self.out_dir.join("wasm_binary.rs");
        if wasm_binary_rs.exists() {
            return wasm_binary_rs;
        }

        let content = r#"#[allow(unused)]
    pub const WASM_BINARY: &[u8] = &[];
    #[allow(unused)]
    pub const WASM_BINARY_OPT: &[u8] = &[];
    "#;
        let path = wasm_binary_rs.as_path();
        fs::write(path, content)
            .unwrap_or_else(|_| panic!("Writing `{}` should not fail!", display_path(path)));
        wasm_binary_rs
    }
}

// Windows has path like `path\to\somewhere` which is incorrect for `include_*`
// Rust's macros
fn display_path<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().display().to_string().replace('\\', "/")
}
