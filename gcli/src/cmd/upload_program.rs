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

//! command `upload_program`
use crate::{result::Result, utils};
use clap::Parser;
use gsdk::signer::Signer;
use std::{fs, path::PathBuf};

/// Deploy program to gear node
#[derive(Parser, Debug)]
pub struct UploadProgram {
    /// gear program code <*.wasm>
    code: PathBuf,
    /// gear program salt ( hex encoding )
    #[arg(short, long, default_value = "0x")]
    salt: String,
    /// gear program init payload ( hex encoding )
    #[arg(short, long, default_value = "0x")]
    payload: String,
    /// gear program gas limit
    ///
    /// if zero, gear will estimate this automatically
    #[arg(short, long, default_value = "0")]
    gas_limit: u64,
    /// gear program balance
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl UploadProgram {
    /// Exec command submit
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let code = fs::read(&self.code)?;
        let payload = utils::hex_to_vec(&self.payload)?;

        let gas = if self.gas_limit == 0 {
            signer
                .calculate_upload_gas(None, code.clone(), payload.clone(), self.value, false, None)
                .await?
                .min_limit
        } else {
            self.gas_limit
        };

        // estimate gas
        let gas_limit = signer.api().cmp_gas_limit(gas)?;

        // upload program
        signer
            .upload_program(
                code,
                utils::hex_to_vec(&self.salt)?,
                payload,
                gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
