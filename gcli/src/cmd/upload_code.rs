// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! command `upload_program`
use crate::app::App;
use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use std::path::PathBuf;
use tokio::{fs, io, io::AsyncReadExt};

/// Upload code to Gear node.
///
/// The code can be then used to deploy programs
/// from it.
#[derive(Clone, Debug, Parser)]
#[group(required = true, multiple = false)]
pub struct UploadCode {
    /// Path to WASM binary.
    ///
    /// Mutually exclusive with `--stdin`.
    path: Option<PathBuf>,

    /// Read WASM binary from stdin.
    ///
    /// Mutually exclusive with providing a path.
    #[arg(short, long)]
    stdin: bool,
}

impl UploadCode {
    /// Exec command submit
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let api = app.signed_api().await?;

        let code = match self {
            Self {
                path: Some(path),
                stdin: false,
            } => fs::read(path)
                .await
                .context("failed to read code from file")?,
            Self {
                path: None,
                stdin: true,
            } => {
                let mut code = Vec::new();
                io::stdin()
                    .read_to_end(&mut code)
                    .await
                    .context("failed to code from stdin")?;
                code
            }
            _ => unreachable!(),
        };

        let code_id = api.upload_code(code).await?.value;

        println!("Successfully uploaded the code");
        println!();
        println!("{} {}", "Code ID:".bold(), code_id);

        Ok(())
    }
}
