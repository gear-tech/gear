// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! command `update`
use crate::result::Result;
use clap::Parser;
use std::process::{self, Command};

const REPO: &str = "https://github.com/gear-tech/gear-program";

/// Update self from crates.io or github
#[derive(Debug, Parser)]
pub struct Update {
    /// Force update self from <https://github.com/gear-tech/gear-program>
    #[arg(short, long)]
    pub force: bool,
}

impl Update {
    /// exec command update
    pub async fn exec(&self) -> Result<()> {
        let args: &[&str] = if self.force {
            &["--git", REPO, "--force"]
        } else {
            &[env!("CARGO_PKG_NAME")]
        };

        if !Command::new("cargo")
            .args([&["install"], args].concat())
            .status()?
            .success()
        {
            process::exit(1);
        }

        Ok(())
    }
}
