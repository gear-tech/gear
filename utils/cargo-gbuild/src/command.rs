// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

// NOTE: Gathering everything in this single file atm since we only have
// one command for now.

use anyhow::{anyhow, Result};
use clap::Parser;
use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

const TEMPLATE_REPO: &str = "https://github.com/MedovTimur/template.git";

/// `cargo-gbuild` commands
#[derive(Parser, Clone)]
pub enum Command {
    /// Create a new gear program
    New {
        /// Path of the gear program to be created
        path: PathBuf,
    },
}

impl Command {
    /// Run commands
    pub fn run(self) -> Result<()> {
        match self {
            Command::New { path } => git_clone(TEMPLATE_REPO, path),
        }
    }
}

/// Clone git repo to the target directory.
fn git_clone(repo: &str, target: PathBuf) -> Result<()> {
    let path = target
        .as_os_str()
        .to_str()
        .ok_or(anyhow!("Failed to convert target path to string"))?;

    // Clone template to the target path.
    process::Command::new("git")
        .args(["clone", &repo, &path, "--depth=1"])
        .status()
        .map_err(|e| anyhow!("Failed to download template: {}", e))?;

    fs::remove_dir(target.join(".git"))?;

    // Init new git repo.
    process::Command::new("git")
        .args(["init", &path])
        .status()
        .map_err(|e| anyhow!("Failed to init git: {}", e))?;
    Ok(())
}