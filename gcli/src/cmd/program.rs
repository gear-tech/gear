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
use crate::{meta::Meta, result::Result};
use clap::Parser;
use gsdk::{ext::sp_core::H256, Api};
use std::{fs, path::PathBuf};

/// Read program state, etc.
#[derive(Clone, Debug, Parser)]
pub enum Program {
    /// Display metadata of the program.
    ///
    /// More details please check https://wiki.gear-tech.io/docs/api/metadata-type-creation.
    Meta {
        /// Path of "*.meta.txt" or "*meta.wasm".
        ///
        /// - "*.meta.txt" describes the metadata of the program
        /// - "*.meta.wasm" describes the wasm exports of the program
        meta: PathBuf,
        /// Derive the description of the specified type from registry.
        #[arg(short, long)]
        derive: Option<String>,
    },
    /// Read program state.
    ///
    /// For more details, see https://wiki.gear-tech.io/docs/api/read-state.
    State {
        /// Program id.
        pid: H256,
        /// Method of reading state from wasm (hex encoding).
        #[arg(short, long)]
        method: Option<String>,
        /// The path of "*.meta.wasm".
        #[arg(short, long)]
        wasm: Option<Vec<u8>>,
        /// Method arugments (hex encoding).
        #[arg(short, long)]
        args: Option<Vec<u8>>,
        /// The block hash for reading state.
        #[arg(long)]
        at: Option<H256>,
    },
}

impl Program {
    /// Run command program.
    pub async fn exec(&self, api: Api) -> Result<()> {
        match self {
            Program::State {
                pid,
                method,
                wasm,
                args,
                at,
            } => {
                if let (Some(wasm), Some(method)) = (wasm, method) {
                    // read state from wasm.
                    Self::wasm_state(api, *pid, wasm.to_vec(), method, args.clone(), *at).await?;
                } else {
                    // read full state
                    Self::full_state(api, *pid, *at).await?;
                }
            }
            Program::Meta { meta, derive } => Self::meta(meta, derive)?,
        }

        Ok(())
    }

    async fn wasm_state(
        api: Api,
        pid: H256,
        wasm: Vec<u8>,
        method: &str,
        args: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<()> {
        let state = api
            .read_state_using_wasm(pid, method, wasm, args, at)
            .await?;
        println!("{}", state);
        Ok(())
    }

    async fn full_state(api: Api, pid: H256, at: Option<H256>) -> Result<()> {
        let state = api.read_state(pid, at).await?;
        println!("{}", state);
        Ok(())
    }

    /// Display meta.
    fn meta(path: &PathBuf, name: &Option<String>) -> Result<()> {
        let ext = path
            .extension()
            .ok_or_else(|| anyhow::anyhow!("Invalid file extension"))?;
        let data = fs::read(path)?;

        // parse fom hex if end with `txt`.
        let meta = if ext == "txt" {
            Meta::decode_hex(&data)?
        } else if ext == "wasm" {
            // parse fom wasm if end with `wasm`.
            Meta::decode_wasm(&data)?
        } else {
            return Err(anyhow::anyhow!(format!("Unsupported file extension {:?}", ext)).into());
        };

        // Format types.
        let fmt = if let Some(name) = name {
            format!("{:#}", meta.derive(name)?)
        } else {
            format!("{:#}", meta)
        };

        // println result.
        println!("{}", fmt.replace('"', ""));
        Ok(())
    }
}
