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

//! command `upload_program`
use crate::{result::Result, utils::Hex};
use anyhow::anyhow;
use clap::Parser;
use gsdk::{
    metadata::{gear::Event as GearEvent, runtime_types::gear_common::event::MessageEntry},
    signer::Signer,
    Event,
};
use std::{fs, path::PathBuf};

/// Deploy program to gear node or save program `code` in storage.
#[derive(Parser, Debug)]
pub struct Upload {
    /// Gear program code <*.wasm>.
    #[cfg_attr(feature = "embed", clap(skip))]
    code: PathBuf,
    /// Overrided code if feature embed is enabled.
    #[clap(skip)]
    code_override: Vec<u8>,
    /// Save program `code` in storage only.
    #[arg(short, long)]
    code_only: bool,
    /// Randomness term (a seed) to allow programs with identical code to be created independently.
    #[arg(short, long, default_value = "0x")]
    salt: String,
    /// Encoded parameters of the wasm module `init` function.
    #[arg(short, long, default_value = "0x")]
    payload: String,
    /// Maximum amount of gas the program can spend before it is halted.
    #[arg(short, long, default_value = "0")]
    gas_limit: u64,
    /// Balance to be transferred to the program once it's been created.
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl Upload {
    /// Override code path.
    pub fn override_code(mut self, code: Vec<u8>) -> Self {
        self.code_override = code;
        self
    }

    /// Exec command submit
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let code = if self.code_override.is_empty() {
            fs::read(&self.code).map_err(|e| anyhow!("program {:?} not found, {e}", &self.code))?
        } else {
            self.code_override.clone()
        };

        if self.code_only {
            signer.calls.upload_code(code).await?;
            return Ok(());
        }

        let payload = self.payload.to_vec()?;
        let gas = if self.gas_limit == 0 {
            signer
                .rpc
                .calculate_upload_gas(None, code.clone(), payload.clone(), self.value, false, None)
                .await?
                .min_limit
        } else {
            self.gas_limit
        };

        // Estimate gas and upload program.
        let gas_limit = signer.api().cmp_gas_limit(gas)?;
        let tx = signer
            .calls
            .upload_program(code, self.salt.to_vec()?, payload, gas_limit, self.value)
            .await?;

        for event in signer.api().events_of(&tx).await? {
            match event {
                Event::Gear(GearEvent::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => {
                    log::info!("Program ID: 0x{}", hex::encode(destination.0));
                    log::info!("Init Message ID: 0x{}", hex::encode(id.0));
                }
                Event::Gear(GearEvent::CodeChanged { id, .. }) => {
                    log::info!("Code ID: 0x{}", hex::encode(id.0));
                }
                _ => {}
            }
        }

        Ok(())
    }
}
