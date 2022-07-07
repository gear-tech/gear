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

use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use toml::value::Table;

use crate::{crate_info::CrateInfo, optimize::Optimizer};

/// Temporary project generated to build a WASM output.
///
/// This project is required due to the cargo locking during build.
pub struct WasmProject {
    original_dir: PathBuf,
    out_dir: PathBuf,
    target_dir: PathBuf,
    file_base_name: Option<String>,
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

        let mut target_dir = out_dir.clone();
        while target_dir.pop() {
            if target_dir.ends_with("target") {
                break;
            }
        }
        target_dir.push("wasm-projects");

        WasmProject {
            original_dir,
            out_dir,
            target_dir,
            file_base_name: None,
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

        let mut release_profile = Table::new();
        release_profile.insert("lto".into(), true.into());
        release_profile.insert("opt-level".into(), "s".into());

        let mut profile = Table::new();
        profile.insert("dev".into(), release_profile.clone().into());
        profile.insert("release".into(), release_profile.into());

        let mut crate_package = Table::new();
        crate_package.insert("package".into(), crate_info.name.into());
        crate_package.insert(
            "path".into(),
            self.original_dir.display().to_string().into(),
        );
        crate_package.insert("default-features".into(), false.into());

        let mut dependencies = Table::new();
        dependencies.insert("orig-project".into(), crate_package.into());

        let mut cargo_toml = Table::new();
        cargo_toml.insert("package".into(), package.into());
        cargo_toml.insert("lib".into(), lib.into());
        cargo_toml.insert("dependencies".into(), dependencies.into());
        cargo_toml.insert("profile".into(), profile.into());
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
        let _ = fs::copy(&from_lock, &to_lock);

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
            .join("wasm32-unknown-unknown/release")
            .join(format!("{}.wasm", &file_base_name));

        fs::create_dir_all(&self.target_dir)?;

        let to_path = self.target_dir.join(format!("{}.wasm", &file_base_name));
        fs::copy(&from_path, &to_path).context("unable to copy WASM file")?;

        let to_opt_path = self
            .target_dir
            .join(format!("{}.opt.wasm", &file_base_name));

        let _ = crate::optimize::optimize_wasm(to_path.clone(), "s", false);

        Self::generate_opt(from_path.clone(), &to_opt_path)?;

        let to_meta_path = self
            .target_dir
            .join(format!("{}.meta.wasm", &file_base_name));
        Self::generate_meta(from_path, &to_meta_path)?;

        let wasm_binary_path = self.original_dir.join(".binpath");

        let mut relative_path = pathdiff::diff_paths(&to_path, &self.original_dir)
            .expect("Unable to calculate relative path");

        // Remove extension
        relative_path.set_extension("");

        fs::write(&wasm_binary_path, format!("{}", relative_path.display()))
            .context("unable to write `.binpath`")?;

        let wasm_binary_rs = self.out_dir.join("wasm_binary.rs");
        fs::write(
            &wasm_binary_rs,
            format!(
                r#"#[allow(unused)]
pub const WASM_BINARY: &[u8] = include_bytes!("{}");
#[allow(unused)]
pub const WASM_BINARY_OPT: &[u8] = include_bytes!("{}");
#[allow(unused)]
pub const WASM_BINARY_META: &[u8] = include_bytes!("{}");
"#,
                display_path(to_path),
                display_path(to_opt_path),
                display_path(to_meta_path),
            ),
        )
        .context("unable to write `wasm_binary.rs`")?;

        Ok(())
    }

    fn generate_opt(from: PathBuf, to: &Path) -> Result<()> {
        let mut optimizer = Optimizer::new(from)?;
        optimizer.insert_stack_and_export();
        let code = optimizer.optimize()?;
        fs::write(to, code)?;
        Ok(())
    }

    fn generate_meta(from: PathBuf, to: &Path) -> Result<()> {
        let mut optimizer = Optimizer::new(from)?;
        optimizer.insert_stack_and_export();
        let code = optimizer.metadata()?;
        fs::write(to, code)?;
        Ok(())
    }
}

// Windows has path like `path\to\somewhere` which is incorrect for `include_*` Rust's macros
fn display_path(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "/")
}
