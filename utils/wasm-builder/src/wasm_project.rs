// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
    optimize::{OptType, Optimizer},
    smart_fs,
};
use anyhow::{Context, Result};
use gmeta::MetadataRepr;
use pwasm_utils::parity_wasm::{self, elements::Internal};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use toml::value::Table;

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
        }
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
        dev_profile.insert("opt-level".into(), "s".into());

        let mut release_profile = Table::new();
        release_profile.insert("lto".into(), true.into());
        release_profile.insert("opt-level".into(), "s".into());

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

        let mut cargo_toml = Table::new();
        cargo_toml.insert("package".into(), package.into());
        cargo_toml.insert("lib".into(), lib.into());
        cargo_toml.insert("dependencies".into(), dependencies.into());
        cargo_toml.insert("profile".into(), profile.into());
        cargo_toml.insert("features".into(), features.into());
        cargo_toml.insert("workspace".into(), Table::new().into());

        fs::write(self.manifest_path(), toml::to_string_pretty(&cargo_toml)?)?;

        let src_dir = self.out_dir.join("src");
        fs::create_dir_all(&src_dir)?;
        fs::write(
            src_dir.join("lib.rs"),
            "#![no_std] pub use orig_project::*;",
        )?;

        // Copy original `Cargo.lock` if any
        let from_lock = self.original_dir.join("Cargo.lock");
        let to_lock = self.out_dir.join("Cargo.lock");
        let _ = fs::copy(from_lock, to_lock);

        // Write metadata
        if let Some(metadata) = &self.project_type.metadata() {
            let file_base_name = self
                .file_base_name
                .as_ref()
                .expect("Run `WasmProject::create_project()` first");

            let wasm_meta_path = self
                .original_dir
                .join([file_base_name, ".meta.txt"].concat());
            let wasm_meta_hash_path = self.original_dir.join(".metahash");

            smart_fs::write_metadata(wasm_meta_path, metadata)
                .context("unable to write `*.meta.txt`")?;

            smart_fs::write(wasm_meta_hash_path, format!("{:?}", metadata.hash()))
                .context("unable to write `.metahash`")?;
        }

        Ok(())
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
            .expect("Run `WasmProject::create_project()` first");

        let from_path = self
            .target_dir
            .join(format!("wasm32-unknown-unknown/{}", self.profile))
            .join(format!("{}.wasm", &file_base_name));

        fs::create_dir_all(&self.target_dir)?;
        fs::create_dir_all(&self.wasm_target_dir)?;

        let [to_path, to_opt_path, to_meta_path] = [".wasm", ".opt.wasm", ".meta.wasm"]
            .map(|ext| self.wasm_target_dir.join([file_base_name, ext].concat()));

        // Optimize source.
        if !self.project_type.is_metawasm() {
            fs::copy(&from_path, &to_path).context("unable to copy WASM file")?;
            let _ = crate::optimize::optimize_wasm(to_path.clone(), "s", false);
        }

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

        // Generate wasm binaries
        Self::generate_wasm(
            from_path,
            (!self.project_type.is_metawasm()).then_some(&to_opt_path),
            self.project_type.is_metawasm().then_some(&to_meta_path),
        )?;

        let wasm_binary_path = self.original_dir.join(".binpath");

        let mut relative_path = pathdiff::diff_paths(&to_path, &self.original_dir)
            .expect("Unable to calculate relative path");

        // Remove extension
        relative_path.set_extension("");

        if !self.project_type.is_metawasm() {
            smart_fs::write(wasm_binary_path, format!("{}", relative_path.display()))
                .context("unable to write `.binpath`")?;
        }

        let wasm_binary_rs = self.out_dir.join("wasm_binary.rs");

        if !self.project_type.is_metawasm() {
            fs::write(
                wasm_binary_rs,
                format!(
                    r#"#[allow(unused)]
pub const WASM_BINARY: &[u8] = include_bytes!("{}");
#[allow(unused)]
pub const WASM_BINARY_OPT: &[u8] = include_bytes!("{}");
{}
"#,
                    display_path(to_path),
                    display_path(to_opt_path),
                    metadata,
                ),
            )
            .context("unable to write `wasm_binary.rs`")?;
        } else {
            fs::write(
                wasm_binary_rs,
                format!(
                    r#"#[allow(unused)]
pub const WASM_BINARY: &[u8] = include_bytes!("{}");
#[allow(unused)]
pub const WASM_EXPORTS: &[&str] = &{:?};

"#,
                    display_path(to_meta_path.clone()),
                    Self::get_exports(&to_meta_path)?,
                ),
            )
            .context("unable to write `wasm_binary.rs`")?;
        }

        Ok(())
    }

    fn generate_wasm(from: PathBuf, to_opt: Option<&Path>, to_meta: Option<&Path>) -> Result<()> {
        let mut optimizer = Optimizer::new(from)?;
        optimizer.insert_stack_and_export();
        optimizer.strip_custom_sections();

        // Generate *.opt.wasm.
        if let Some(to_opt) = to_opt {
            let opt = optimizer.optimize(OptType::Opt)?;
            fs::write(to_opt, opt)?;
        }

        // Generate *.meta.wasm.
        if let Some(to_meta) = to_meta {
            let meta = optimizer.optimize(OptType::Meta)?;
            fs::write(to_meta, meta)?;
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
}

// Windows has path like `path\to\somewhere` which is incorrect for `include_*` Rust's macros
fn display_path(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "/")
}
