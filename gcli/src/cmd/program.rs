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

//! Command `program`.
//!
use crate::{meta::Meta, result::Result, App};
use anyhow::anyhow;
use clap::Parser;
use gclient::{ext::sp_core::H256, GearApi};
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
        #[cfg_attr(feature = "embed", clap(skip))]
        meta: PathBuf,
        /// Overridden metadata binary if feature embed is enabled.
        #[clap(skip)]
        meta_override: Vec<u8>,
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
        /// The block hash for reading state.
        #[arg(long)]
        at: Option<H256>,
    },
}

impl Program {
    /// Clone self with metadata overridden.
    pub fn clone_with_meta_overridden(&self, meta: Vec<u8>) -> Self {
        let mut overridden = self.clone();
        if let Program::Meta { meta_override, .. } = &mut overridden {
            *meta_override = meta;
        };
        overridden
    }

    /// Run command program.
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        match self {
            Program::State { pid, at } => {
                let api = app.signer().await?;
                Self::full_state(&api, *pid, *at).await?;
            }
            Program::Meta {
                meta,
                derive,
                meta_override,
            } => {
                let meta = if meta_override.is_empty() {
                    Self::resolve_meta(meta)
                } else {
                    Meta::decode_wasm(meta_override)
                }?;

                Self::meta(meta, derive)?
            }
        }

        Ok(())
    }

    async fn full_state(api: &GearApi, pid: H256, at: Option<H256>) -> Result<()> {
        let state = api
            .read_state_bytes_at(pid.0.into(), Default::default(), at)
            .await?;
        println!("0x{}", hex::encode(state));
        Ok(())
    }

    fn resolve_meta(path: &PathBuf) -> Result<Meta> {
        let ext = path
            .extension()
            .ok_or_else(|| anyhow!("Invalid file extension"))?;
        let data = fs::read(path)?;

        // parse from hex if end with `txt`.
        let meta = if ext == "txt" {
            Meta::decode_hex(&data)?
        } else if ext == "wasm" {
            // parse from wasm if end with `wasm`.
            Meta::decode_wasm(&data)?
        } else {
            return Err(anyhow!(format!("Unsupported file extension {:?}", ext)).into());
        };

        Ok(meta)
    }

    fn meta(meta: Meta, name: &Option<String>) -> Result<()> {
        let fmt = if let Some(name) = name {
            format!("{:#}", meta.derive(name)?)
        } else {
            format!("{meta:#}")
        };

        // println result.
        println!("{}", fmt.replace('"', ""));
        Ok(())
    }
}
