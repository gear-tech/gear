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

//! Command `program`.
use crate::{metadata::Metadata, result::Result, utils};
use clap::Parser;
use gsdk::{ext::sp_core::H256, Api};
use std::{fs, path::PathBuf};

/// Read program state, etc.
#[derive(Clone, Debug, Parser)]
pub enum Action {
    /// Read program state.
    State {
        /// Path of "*.meta.wasm".
        metadata: PathBuf,
        /// Input message for reading program state.
        #[arg(short, long, default_value = "0x")]
        msg: String,
        /// Block timestamp.
        #[arg(short, long)]
        timestamp: Option<u64>,
        /// Block height.
        #[arg(long)]
        height: Option<u64>,
    },
}

/// Read program state, etc.
#[derive(Debug, Parser)]
pub struct Program {
    /// Program id.
    pid: String,
    #[command(subcommand)]
    action: Action,
}

impl Program {
    /// Run command program.
    pub async fn exec(&self, api: Api) -> Result<()> {
        let pid_bytes = hex::decode(self.pid.trim_start_matches("0x"))?;
        let mut pid = [0; 32];
        pid.copy_from_slice(&pid_bytes);

        match self.action {
            Action::State { .. } => self.state(api, pid.into()).await?,
        }

        Ok(())
    }

    /// Read program state.
    pub async fn state(&self, api: Api, pid: H256) -> Result<()> {
        let Action::State {
            metadata,
            msg,
            timestamp,
            height,
        } = self.action.clone();

        // Get program
        let program = api.gprog(pid.0.into()).await?;
        let code_id = program.code_hash;
        let code = api.code_storage(code_id.0.into()).await?;
        let pages = api.gpages(pid.0.into(), &program).await?;

        // Query state
        let state = Metadata::read(
            &fs::read(&metadata)?,
            code.static_pages.0 as u64,
            pages,
            utils::hex_to_vec(msg)?,
            timestamp.unwrap_or(0),
            height.unwrap_or(0),
        )?;

        println!("state: 0x{}", hex::encode(state));

        Ok(())
    }
}
