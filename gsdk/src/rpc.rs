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

//! gear api rpc methods
#![allow(clippy::too_many_arguments)]
use crate::{result::Result, types::GasInfo, Api};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use sp_core::H256;
use subxt::rpc::rpc_params;

impl Api {
    /// gear_calculateInitCreateGas
    pub async fn calculate_create_gas(
        &self,
        origin: H256,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitCreateGas",
                rpc_params![
                    origin,
                    H256(code_id.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateInitUploadGas
    pub async fn calculate_upload_gas(
        &self,
        origin: H256,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitUploadGas",
                rpc_params![
                    origin,
                    hex::encode(code),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateHandleGas
    pub async fn calculate_handle_gas(
        &self,
        origin: H256,
        destination: ProgramId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateHandleGas",
                rpc_params![
                    origin,
                    H256(destination.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateReplyGas
    pub async fn calculate_reply_gas(
        &self,
        origin: H256,
        message_id: MessageId,
        exit_code: i32,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateReplyGas",
                rpc_params![
                    origin,
                    H256(message_id.into()),
                    exit_code,
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_meta_hash
    pub async fn read_meta_hash(&self, pid: H256, at: Option<H256>) -> Result<H256> {
        self.rpc()
            .request("gear_readMetahash", rpc_params![H256(pid.into()), at])
            .await
            .map_err(Into::into)
    }

    /// gear_readState
    pub async fn read_state(&self, pid: H256, at: Option<H256>) -> Result<String> {
        self.rpc()
            .request("gear_readState", rpc_params![H256(pid.into()), at])
            .await
            .map_err(Into::into)
    }

    /// gear_readStateUsingWasm
    pub async fn read_state_using_wasm(
        &self,
        pid: H256,
        method: &str,
        wasm: Vec<u8>,
        args: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<String> {
        self.rpc()
            .request(
                "gear_readStateUsingWasm",
                rpc_params![
                    pid,
                    hex::encode(method),
                    hex::encode(wasm),
                    args.map(hex::encode),
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }
}
