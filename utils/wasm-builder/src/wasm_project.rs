// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

use crate::{
    crate_info::CrateInfo,
    optimize::{self, OptType, Optimizer},
    smart_fs,
};
use anyhow::{Context, Result};
use chrono::offset::Local as ChronoLocal;
use gmeta::MetadataRepr;
use pwasm_utils::parity_wasm::{self, elements::Internal};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use toml::value::Table;

const OPT_LEVEL: &str = "z";

/// Enum defining type of binary compiling: production program or metawasm.
pub enum ProjectType {
    Program(Option<MetadataRepr>),
    Metawasm,
}

impl ProjectType {
    pub fn is_metawasm(&self) -> bool {
        matches!(self, ProjectType::Metawasm)
    }

    pub fn metadata(&self) -> Option<&MetadataRepr> {
        match self {
            ProjectType::Program(metadata) => metadata.as_ref(),
            _ => None,
        }
    }
}

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
    project_type: ProjectType,
    features: Option<Vec<String>>,
}

impl WasmProject {
    /// Create a new `WasmProject`.
    pub fn new(project_type: ProjectType) -> Self {
        let original_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")
            .expect("`CARGO_MANIFEST_DIR` is always set in build scripts")
            .into();

        let out_dir: PathBuf = env::var("OUT_DIR")
            .expect("`OUT_DIR` is always set in build scripts")
            .into();

        let profile = out_dir
            .components()
            .rev()
            .take_while(|c| c.as_os_str() != "target")
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take_while(|c| c.as_os_str() != "build")
            .last()
            .expect("Path should have subdirs in the `target` dir")
            .as_os_str()
            .to_string_lossy()
            .into();

        let mut target_dir = out_dir.clone();
        while target_dir.pop() {
            if target_dir.ends_with("target") {
                break;
            }
        }

        let mut wasm_target_dir = target_dir.clone();
        wasm_target_dir.push("wasm32-unknown-unknown");
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
            project_type,
            features: None,
        }
    }

    /// Return the path to the temporary generated `Cargo.toml`.
    pub fn manifest_path(&self) -> PathBuf {
        self.out_dir.join("Cargo.toml")
    }

    /// Return the path to the original project directory.
    pub fn original_dir(&self) -> PathBuf {
        self.original_dir.clone()
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

    /// Generate a temporary cargo project that includes the original package as a dependency.
    pub fn generate(&mut self) -> Result<()> {
        let original_manifest = self.original_dir.join("Cargo.toml");
        let crate_info = CrateInfo::from_manifest(&original_manifest)?;
        self.file_base_name = Some(crate_info.snake_case_name.clone());

        let mut package = Table::new();
        package.insert("name".into(), format!("{}-wasm", &crate_info.name).into());
        package.insert("version".into(), crate_info.version.into());
        package.insert("edition".into(), "2021".into());

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

        let mut profile = Table::new();
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

        smart_fs::write(self.manifest_path(), toml::to_string_pretty(&cargo_toml)?)?;

        // Copy original `Cargo.lock` if any
        let from_lock = self.original_dir.join("Cargo.lock");
        let to_lock = self.out_dir.join("Cargo.lock");
        let _ = fs::copy(from_lock, to_lock);

        let mut source_code = "#![no_std] pub use orig_project::*;\n".to_owned();

        // Write metadata
        if let Some(metadata) = &self.project_type.metadata() {
            let file_base_name = self
                .file_base_name
                .as_ref()
                .expect("Run `WasmProject::create_project()` first");

            let wasm_meta_path = self
                .original_dir
                .join([file_base_name, ".meta.txt"].concat());

            smart_fs::write_metadata(wasm_meta_path, metadata)
                .context("unable to write `*.meta.txt`")?;

            source_code = format!(
                r#"{source_code}
#[allow(improper_ctypes)]
mod fake_gsys {{
    extern "C" {{
        pub fn gr_reply(
            payload: *const u8,
            len: u32,
            value: *const u128,
            err_mid: *mut [u8; 36],
        );
    }}
}}

#[no_mangle]
extern "C" fn metahash() {{
    const METAHASH: [u8; 32] = {:?};
    let mut res: [u8; 36] = [0; 36];
    unsafe {{
        fake_gsys::gr_reply(
            METAHASH.as_ptr(),
            METAHASH.len() as _,
            u32::MAX as _,
            &mut res as _,
        );
    }}
}}
"#,
                metadata.hash(),
            );
        }

        let src_dir = self.out_dir.join("src");
        fs::create_dir_all(&src_dir)?;
        smart_fs::write(src_dir.join("lib.rs"), source_code)?;

        Ok(())
    }

    /// Generate output wasm meta file and wasm binary informational file.
    pub fn postprocess_meta(
        &self,
        original_wasm_path: &PathBuf,
        file_base_name: &String,
    ) -> Result<()> {
        let meta_wasm_path = self
            .wasm_target_dir
            .join([file_base_name, ".meta.wasm"].concat());

        if smart_fs::check_if_newer(original_wasm_path, &meta_wasm_path)? {
            fs::write(
                meta_wasm_path.clone(),
                Optimizer::new(original_wasm_path.clone())?.optimize(OptType::Meta)?,
            )?;
        }

        smart_fs::write(
            self.out_dir.join("wasm_binary.rs"),
            format!(
                r#"#[allow(unused)]
                       pub const WASM_BINARY: &[u8] = include_bytes!("{}");
                       #[allow(unused)]
                       pub const WASM_EXPORTS: &[&str] = &{:?};"#,
                display_path(meta_wasm_path.clone()),
                Self::get_exports(&meta_wasm_path)?,
            ),
        )
        .context("unable to write `wasm_binary.rs`")
        .map_err(Into::into)
    }

    /// Generates output optimized wasm file, `.binpath` file for our tests system
    /// and wasm binaries informational file.
    /// Makes a copy of original wasm file in `self.wasm_target_dir`.
    pub fn postprocess_opt(
        &self,
        original_wasm_path: &PathBuf,
        file_base_name: &String,
    ) -> Result<()> {
        let [original_copy_wasm_path, opt_wasm_path] = [".wasm", ".opt.wasm"]
            .map(|ext| self.wasm_target_dir.join([file_base_name, ext].concat()));

        // Copy original file to `self.wasm_target_dir`
        smart_fs::copy_if_newer(original_wasm_path, &original_copy_wasm_path)
            .context("unable to copy WASM file")?;

        // Optimize wasm using and `wasm-opt` and our optimizations.
        if smart_fs::check_if_newer(original_wasm_path, &opt_wasm_path)? {
            let path = optimize::optimize_wasm(
                original_copy_wasm_path.clone(),
                opt_wasm_path.clone(),
                "4",
                true,
            )
            .map(|res| {
                log::info!(
                    "Wasm-opt reduced wasm size: {} -> {}",
                    res.original_size,
                    res.optimized_size
                );
                opt_wasm_path.clone()
            })
            .unwrap_or_else(|err| {
                println!("cargo:warning=wasm-opt optimizations error: {}", err);
                original_copy_wasm_path.clone()
            });

            let mut optimizer = Optimizer::new(path)?;
            optimizer
                .insert_stack_end_export()
                .unwrap_or_else(|err| log::info!("Cannot insert stack end export: {}", err));
            optimizer.strip_custom_sections();
            fs::write(opt_wasm_path.clone(), optimizer.optimize(OptType::Opt)?)?;
        }

        // Create path string in `.binpath` file.
        let relative_path_to_wasm = pathdiff::diff_paths(&self.wasm_target_dir, &self.original_dir)
            .expect("Unable to calculate relative path")
            .join(file_base_name);
        smart_fs::write(
            self.original_dir.join(".binpath"),
            format!("{}", relative_path_to_wasm.display()),
        )
        .context("unable to write `.binpath`")?;

        // Create `wasm_binary.rs`
        let metadata = self
            .project_type
            .metadata()
            .map(|m| {
                format!(
                    "#[allow(unused)] pub const WASM_METADATA: &[u8] = &{:?};\n",
                    m.bytes()
                )
            })
            .unwrap_or_default();
        smart_fs::write(
            self.out_dir.join("wasm_binary.rs"),
            format!(
                r#"#[allow(unused)]
                       pub const WASM_BINARY: &[u8] = include_bytes!("{}");
                       #[allow(unused)]
                       pub const WASM_BINARY_OPT: &[u8] = include_bytes!("{}");
                       {}"#,
                display_path(original_copy_wasm_path),
                display_path(opt_wasm_path),
                metadata,
            ),
        )
        .context("unable to write `wasm_binary.rs`")
    }

    /// Post-processing after the WASM binary has been built.
    ///
    /// - Copy WASM binary from `OUT_DIR` to `target/wasm32-unknown-unknown/<profile>`
    /// - Generate optimized and metadata WASM binaries from the built program
    /// - Generate `wasm_binary.rs` source file in `OUT_DIR`
    pub fn postprocess(&self) -> Result<()> {
        let file_base_name = self
            .file_base_name
            .as_ref()
            .expect("Run `WasmProject::generate()` first");

        let original_wasm_path = self
            .target_dir
            .join(format!("wasm32-unknown-unknown/{}", self.profile))
            .join(format!("{}.wasm", &file_base_name));

        fs::create_dir_all(&self.target_dir)?;
        fs::create_dir_all(&self.wasm_target_dir)?;

        if self.project_type.is_metawasm() {
            self.postprocess_meta(&original_wasm_path, file_base_name)?;
        } else {
            self.postprocess_opt(&original_wasm_path, file_base_name)?;
        }

        if env::var("__GEAR_WASM_BUILDER_NO_FEATURES_TRACKING").is_err() {
            self.force_rerun_on_next_run(&original_wasm_path)?;
        }

        Ok(())
    }

    fn get_exports(file: &PathBuf) -> Result<Vec<String>> {
        let module =
            parity_wasm::deserialize_file(file).with_context(|| format!("File path: {file:?}"))?;

        let exports = module
            .export_section()
            .ok_or_else(|| anyhow::anyhow!("Export section not found"))?
            .entries()
            .iter()
            .flat_map(|entry| {
                if let Internal::Function(_) = entry.internal() {
                    Some(entry.field().to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(exports)
    }

    // Force cargo to re-run the build script next time it is invoked.
    // It is needed because feature set or toolchain can change.
    fn force_rerun_on_next_run(&self, wasm_file_path: &Path) -> Result<()> {
        let stamp_file_path = wasm_file_path.with_extension("stamp");
        fs::write(&stamp_file_path, ChronoLocal::now().to_rfc3339())?;
        println!("cargo:rerun-if-changed={}", stamp_file_path.display());
        Ok(())
    }

    /// Provide a dummy WASM binary if there doesn't exist one.
    pub fn provide_dummy_wasm_binary_if_not_exist(&self) {
        let wasm_binary_rs = self.out_dir.join("wasm_binary.rs");
        if wasm_binary_rs.exists() {
            return;
        }

        let content = if !self.project_type.is_metawasm() {
            r#"#[allow(unused)]
    pub const WASM_BINARY: &[u8] = &[];
    #[allow(unused)]
    pub const WASM_BINARY_OPT: &[u8] = &[];
    #[allow(unused)] pub const WASM_METADATA: &[u8] = &[];
    "#
        } else {
            r#"#[allow(unused)]
    pub const WASM_BINARY: &[u8] = &[];
    #[allow(unused)]
    pub const WASM_EXPORTS: &[&str] = &[];
    "#
        };
        fs::write(wasm_binary_rs.as_path(), content).unwrap_or_else(|_| {
            panic!(
                "Writing `{}` should not fail!",
                display_path(wasm_binary_rs)
            )
        });
    }
}

// Windows has path like `path\to\somewhere` which is incorrect for `include_*` Rust's macros
fn display_path(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "/")
}
