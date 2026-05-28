// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// NOTE: Gathering everything in this single file atm since we only have
// one command for now.

use crate::utils;
use anyhow::{Result, anyhow};
use clap::Parser;
use colored::Colorize;
use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

const TEMPLATE_REPO: &str = "https://github.com/gear-foundation/dapp-template-generate.git";

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

    // clone template to the target path.
    utils::info("Cloning", repo);
    let result = process::Command::new("git")
        .args(["clone", repo, path, "--depth=1"])
        .output()
        .map_err(|e| anyhow!("Failed to download template: {e}"))?;

    if !result.status.success() {
        utils::error(&result.stderr);
    }

    // clean the .git in template.
    fs::remove_dir_all(target.join(".git"))?;

    // init new git.
    process::Command::new("git")
        .args(["init", path])
        .output()
        .map_err(|e| anyhow!("Failed to init git: {e}"))?;
    if !result.status.success() {
        utils::error(&result.stderr);
    }

    utils::info("Created", path);
    Ok(())
}
