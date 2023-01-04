// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
#![allow(clippy::too_many_arguments)]

use crate::{api::Result, GearApi};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gp::api::types::GasInfo;
use parity_scale_codec::{Decode, Encode};
use std::path::Path;
use subxt::{ext::sp_core::H256, rpc::rpc_params};

use crate::utils;

impl GearApi {
    /// gear_calculateInitCreateGas
    pub async fn calculate_create_gas(
        &self,
        origin: Option<H256>,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .calculate_create_gas(origin, code_id, payload, value, allow_other_panics, at)
            .await
            .map_err(Into::into)
    }

    /// gear_calculateInitUploadGas
    pub async fn calculate_upload_gas(
        &self,
        origin: Option<H256>,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .calculate_upload_gas(origin, code, payload, value, allow_other_panics, at)
            .await
            .map_err(Into::into)
    }

    /// gear_calculateHandleGas
    pub async fn calculate_handle_gas(
        &self,
        origin: Option<H256>,
        destination: ProgramId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .calculate_handle_gas(origin, destination, payload, value, allow_other_panics, at)
            .await
            .map_err(Into::into)
    }

    /// gear_calculateReplyGas
    pub async fn calculate_reply_gas(
        &self,
        origin: Option<H256>,
        message_id: MessageId,
        exit_code: i32,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .calculate_reply_gas(
                origin,
                message_id,
                exit_code,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
            .map_err(Into::into)
    }

    /// Read the program's state as a byte vector.
    pub async fn read_state_bytes(
        &self,
        program_id: ProgramId,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        self.0
            .rpc()
            .request("gear_readState", rpc_params![H256(program_id.into()), at])
            .await
            .map_err(Into::into)
    }

    /// Read the program's state as decoded data.
    pub async fn read_state<D: Decode>(
        &self,
        program_id: ProgramId,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self.read_state_bytes(program_id, at).await?;
        D::decode(&mut bytes.as_ref()).map_err(Into::into)
    }

    /// Read the program's state as a byte vector using a meta Wasm.
    pub async fn read_state_bytes_using_wasm(
        &self,
        program_id: ProgramId,
        fn_name: &[u8],
        wasm: Vec<u8>,
        argument: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        self.0
            .rpc()
            .request(
                "gear_readStateUsingWasm",
                rpc_params![
                    H256(program_id.into()),
                    fn_name.to_vec(),
                    wasm,
                    argument,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Read the program's state as decoded data using a meta Wasm.
    pub async fn read_state_using_wasm<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &[u8],
        wasm: Vec<u8>,
        argument: Option<E>,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self
            .read_state_bytes_using_wasm(
                program_id,
                fn_name,
                wasm,
                argument.map(|v| v.encode()),
                at,
            )
            .await?;

        D::decode(&mut bytes.as_ref()).map_err(Into::into)
    }

    /// Read the program's state using a meta Wasm file referenced by its `path`.
    pub async fn read_state_bytes_using_wasm_by_path(
        &self,
        program_id: ProgramId,
        fn_name: &[u8],
        path: impl AsRef<Path>,
        argument: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        let wasm = utils::code_from_os(path.as_ref())?;
        self.0
            .rpc()
            .request(
                "gear_readStateUsingWasm",
                rpc_params![
                    H256(program_id.into()),
                    fn_name.to_vec(),
                    wasm,
                    argument,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Read the program's state using a meta Wasm file referenced by its `path`.
    pub async fn read_state_using_wasm_by_path<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &[u8],
        path: impl AsRef<Path>,
        argument: Option<E>,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self
            .read_state_bytes_using_wasm_by_path(
                program_id,
                fn_name,
                path,
                argument.map(|v| v.encode()),
                at,
            )
            .await?;

        D::decode(&mut bytes.as_ref()).map_err(Into::into)
    }

    /// Read the program's metahash.
    pub async fn read_metahash(&self, program_id: ProgramId, at: Option<H256>) -> Result<H256> {
        self.0
            .rpc()
            .request(
                "gear_readMetahash",
                rpc_params![H256(program_id.into()), at],
            )
            .await
            .map_err(Into::into)
    }
}
