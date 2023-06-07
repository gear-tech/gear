// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Gear program template

use crate::{result::Result, utils};
use anyhow::anyhow;
use etc::{Etc, FileSystem, Read, Write};
use gmeta::BTreeMap;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const GEAR_REPO: &str = "https://github.com/gear-tech/gear";
const CONFIG_TOML: &str = r#"
[build]
target = "wasm32-unknown-unknown"

[target.wasm32-unknown-unknown]
rustflags = [
  "-C", "link-args=--import-memory",
  "-C", "linker-plugin-lto",
]
"#;

/// Template manager
pub struct Template {
    /// Template lists
    list: BTreeMap<String, PathBuf>,
}

impl Template {
    /// Initialize a new template manager.
    pub fn new() -> Result<Self> {
        let repo = utils::home().join("gear");

        Self::fetch(&repo)?;
        let list = Self::list(&repo)?;

        Ok(Self { list })
    }

    /// List all templates.
    pub fn ls(&self) -> Vec<String> {
        self.list.keys().cloned().collect()
    }

    /// Copy example to path.
    pub fn cp(&self, name: &str, to: impl AsRef<Path>) -> Result<()> {
        let from = self
            .list
            .get(name)
            .ok_or_else(|| anyhow!("Invalid example name"))?;

        let to = to.as_ref().into();
        etc::cp_r(from, &to)?;

        let proj = Etc::new(to)?;
        let manifest = proj.open("Cargo.toml")?;
        let mut toml = String::from_utf8_lossy(
            &manifest
                .read()
                .map_err(|_| anyhow!("Failed to read Cargo.toml"))?,
        )
        .to_string();

        // Update `Cargo.toml`.
        Self::process_manifest(&mut toml)?;
        manifest.write(toml.as_bytes())?;

        // Add `config.toml`.
        proj.open(".cargo/config.toml")?
            .write(CONFIG_TOML.trim_start().as_bytes())?;

        Ok(())
    }

    /// Update project manifest.
    fn process_manifest(manifest: &mut String) -> Result<()> {
        let (user, email) = {
            let user_bytes = Command::new("git")
                .args(["config", "--global", "--get", "user.name"])
                .output()?
                .stdout;
            let user = String::from_utf8_lossy(&user_bytes);

            let email_bytes = Command::new("git")
                .args(["config", "--global", "--get", "user.email"])
                .output()?
                .stdout;
            let email = String::from_utf8_lossy(&email_bytes);

            (user.to_string(), email.to_string())
        };

        *manifest = manifest
            .replace(
                "authors.workspace = true",
                &format!("authors = [\"{} <{}>\"]", user.trim(), email.trim()),
            )
            .replace("edition.workspace = true", "edition = \"2021\"")
            .replace("license.workspace = true", "license = \"GPL-3.0\"")
            .replace(
                ".workspace = true",
                &format!(" = {{ git = \"{}\" }}", GEAR_REPO),
            )
            .replace("workspace = true", &format!("git = \"{}\"", GEAR_REPO));

        Ok(())
    }

    /// Clone or update the local gear repo.
    fn fetch(repo: impl AsRef<Path>) -> Result<()> {
        let repo = repo.as_ref();
        if !repo.exists() {
            Command::new("git")
                .args([
                    "clone",
                    GEAR_REPO,
                    repo.to_string_lossy().as_ref(),
                    "--depth=1",
                ])
                .output()
                .map_err(|_| anyhow!("Failed to clone gear repo"))?;
        } else {
            Command::new("git")
                .args([
                    &format!("--git-dir={}", repo.join(".git").to_string_lossy().as_ref()),
                    "pull",
                ])
                .output()
                .map_err(|_| anyhow!("Failed to update gear repo"))?;
        }

        Ok(())
    }

    /// Get all examples.
    fn list(repo: impl AsRef<Path>) -> Result<BTreeMap<String, PathBuf>> {
        let repo = repo.as_ref();
        let templates = fs::read_dir(repo.join("examples"))?;

        let mut map = BTreeMap::new();
        for template in templates {
            let t = template?.file_name();
            let example = t.to_string_lossy();
            if example.contains('.') {
                continue;
            }

            let path = repo.join("examples").join(example.as_ref());
            map.insert(example.to_string(), path);
        }

        Ok(map)
    }
}
