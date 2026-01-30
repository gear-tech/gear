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

//! Command `create-program`.

use crate::{app::App, utils::HexBytes};
use anyhow::{Context, Result};
use clap::{Args, Parser};
use colored::Colorize;
use gear_core::ids::CodeId;
use std::path::PathBuf;
use tokio::{
    fs,
    io::{self, AsyncReadExt},
};

/// Deploy a program to a Gear node.
#[derive(Clone, Debug, Parser)]
pub struct CreateProgram {
    /// Program salt, as hex string.
    ///
    /// Used to create multiple programs with the same code.
    #[arg(short, long, default_value = "0x")]
    salt: HexBytes,

    /// Initial message payload, as hex string.
    ///
    /// Sent to the program during initialization.
    #[arg(short, long, default_value = "0x")]
    init_payload: HexBytes,

    /// Operation gas limit.
    ///
    /// Defaults to the estimated gas limit
    /// required for the operation.
    #[arg(short, long)]
    gas_limit: Option<u64>,

    /// Initial program balance.
    #[arg(short, long, default_value = "0")]
    value: u128,

    #[clap(flatten)]
    code_args: CodeArgs,
}

#[derive(Debug, Clone, Args)]
#[group(required = true, multiple = false)]
struct CodeArgs {
    /// ID of a previously uploaded code.
    ///
    /// Mutually exclusive with `--path` and `--stdin`.
    #[arg(short, long)]
    code_id: Option<CodeId>,

    /// Path to the program code.
    ///
    /// Mutually exclusive with `--code-id` and `--stdin`.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Read the program code from stdin.
    ///
    /// Mutually exclusive with `--code-id` and `--path`.
    #[arg(long)]
    stdin: bool,
}

enum Code {
    Uploaded(CodeId),
    Binary(Vec<u8>),
}

impl CodeArgs {
    async fn into_code(self) -> Result<Code> {
        match self {
            Self {
                code_id: Some(code_id),
                path: None,
                stdin: false,
            } => Ok(Code::Uploaded(code_id)),
            Self {
                code_id: None,
                path: Some(path),
                stdin: false,
            } => Ok(Code::Binary(
                fs::read(path)
                    .await
                    .context("failed to read program code from file")?,
            )),
            Self {
                code_id: None,
                path: None,
                stdin: true,
            } => Ok(Code::Binary({
                let mut buffer = Vec::new();
                io::stdin()
                    .read_to_end(&mut buffer)
                    .await
                    .context("failed to read program code from stdin")?;
                buffer
            })),
            _ => unreachable!(), // `CodeArgs` is only used by `clap`, which validates the input
        }
    }
}

impl CreateProgram {
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let api = app.signed_api().await?;

        let code = self.code_args.into_code().await?;

        let gas_limit = if let Some(gas_limit) = self.gas_limit {
            gas_limit
        } else {
            match &code {
                Code::Uploaded(code_id) => {
                    api.calculate_create_gas(*code_id, &self.init_payload, self.value, false)
                        .await?
                }
                Code::Binary(code) => {
                    api.calculate_upload_gas(code, &self.init_payload, self.value, false)
                        .await?
                }
            }
            .min_limit
        };

        let (message_id, program_id) = match code {
            Code::Uploaded(code_id) => {
                api.create_program_bytes(
                    code_id,
                    self.salt.as_slice(),
                    self.init_payload.as_slice(),
                    gas_limit,
                    self.value,
                )
                .await?
            }
            Code::Binary(code) => {
                api.upload_program_bytes(
                    code,
                    self.salt.as_slice(),
                    self.init_payload.as_slice(),
                    gas_limit,
                    self.value,
                )
                .await?
            }
        }
        .value;

        println!("Successfully deployed the program");
        println!();
        println!("{} {}", "Initial message ID:".bold(), message_id);
        println!("{} {}", "Program ID:".bold(), program_id);

        Ok(())
    }
}
