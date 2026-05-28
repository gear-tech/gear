// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! command `update`
use anyhow::{Context, Result};
use clap::Parser;
use std::process::Command;

const REPO: &str = "https://github.com/gear-tech/gear-program";

/// Update self from crates.io or github
#[derive(Clone, Debug, Parser)]
pub struct Update {
    /// Force update self from <https://github.com/gear-tech/gear-program>
    #[arg(short, long)]
    pub force: bool,
}

impl Update {
    /// exec command update
    pub async fn exec(self) -> Result<()> {
        let args: &[&str] = if self.force {
            &["--git", REPO, "--force"]
        } else {
            &[env!("CARGO_PKG_NAME")]
        };

        let status = Command::new("cargo")
            .args([&["install"], args].concat())
            .status()
            .context("failed to self-update using `cargo install`")?;

        if !status.success() {
            std::process::exit(status.code().unwrap_or(1))
        }

        Ok(())
    }
}
