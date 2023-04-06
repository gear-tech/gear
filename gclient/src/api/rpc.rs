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
use gsdk::{ext::sp_core::H256, types::GasInfo};
use parity_scale_codec::{Decode, Encode};
use std::path::Path;

use crate::utils;

impl GearApi {
    /// Execute an RPC to calculate the gas required to create a program from a
    /// code and process an initialization message.
    ///
    /// Actually sends the `gear_calculateInitCreateGas` RPC to the node. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `code_id` is the uploaded code identifier that can be obtained by
    ///   calling the [`upload_code`](Self::upload_code) function;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    /// - `at` (optional) allows executing the RPC at the specified block
    ///   identified by its hash.
    pub async fn calculate_create_gas(
        &self,
        origin: Option<H256>,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
    ) -> Result<GasInfo> {
        self.calculate_create_gas_at(origin, code_id, payload, value, allow_other_panics, None)
            .await
    }

    /// Same as [`calculate_create_gas`](Self::calculate_create_gas), but
    /// calculates the gas at the block identified by its hash.
    pub async fn calculate_create_gas_at(
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

    /// Execute an RPC to calculate the gas required to upload a program and
    /// process an initialization message.
    ///
    /// Actually sends the `gear_calculateInitUploadGas` RPC to the node. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `code` is the buffer containing the Wasm binary code of the Gear
    ///   program;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    /// - `at` (optional) allows executing the RPC at the specified block
    ///   identified by its hash.
    pub async fn calculate_upload_gas(
        &self,
        origin: Option<H256>,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
    ) -> Result<GasInfo> {
        self.calculate_upload_gas_at(origin, code, payload, value, allow_other_panics, None)
            .await
    }

    /// Same as [`calculate_upload_gas`](Self::calculate_upload_gas), but
    /// calculates the gas at the block identified by its hash.
    pub async fn calculate_upload_gas_at(
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

    /// Execute an RPC to calculate the gas required to handle a message.
    ///
    /// Actually sends the `gear_calculateHandleGas` RPC to the node. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `destination` is the program address;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    /// - `at` (optional) allows executing the RPC at the specified block
    ///   identified by its hash.
    pub async fn calculate_handle_gas(
        &self,
        origin: Option<H256>,
        destination: ProgramId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
    ) -> Result<GasInfo> {
        self.calculate_handle_gas_at(
            origin,
            destination,
            payload,
            value,
            allow_other_panics,
            None,
        )
        .await
    }

    /// Same as [`calculate_handle_gas`](Self::calculate_handle_gas), but
    /// calculates the gas at the block identified by its hash.
    pub async fn calculate_handle_gas_at(
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

    /// Execute an RPC to calculate the gas required to reply to the received
    /// message from the mailbox.
    ///
    /// Actually sends the `gear_calculateReplyGas` RPC to the node. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `message_id` is a message identifier required to find it in the
    ///   mailbox;
    /// - `exit_code` is the status code of the reply;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    /// - `at` (optional) allows executing the RPC at the specified block
    ///   identified by its hash.
    pub async fn calculate_reply_gas(
        &self,
        origin: Option<H256>,
        message_id: MessageId,
        exit_code: i32,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
    ) -> Result<GasInfo> {
        self.calculate_reply_gas_at(
            origin,
            message_id,
            exit_code,
            payload,
            value,
            allow_other_panics,
            None,
        )
        .await
    }

    /// Same as [`calculate_reply_gas`](Self::calculate_reply_gas), but
    /// calculates the gas at the block identified by its hash.
    pub async fn calculate_reply_gas_at(
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
    pub async fn read_state_bytes(&self, program_id: ProgramId) -> Result<Vec<u8>> {
        self.read_state_bytes_at(program_id, None).await
    }

    /// Same as [`read_state_bytes`](Self::read_state_bytes), but reads the
    /// program's state at the block identified by its hash.
    pub async fn read_state_bytes_at(
        &self,
        program_id: ProgramId,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        let response: String = self.0.api().read_state(H256(program_id.into()), at).await?;
        crate::utils::hex_to_vec(response).map_err(Into::into)
    }

    /// Read the program's state as decoded data.
    pub async fn read_state<D: Decode>(&self, program_id: ProgramId) -> Result<D> {
        self.read_state_at(program_id, None).await
    }

    /// Same as [`read_state`](Self::read_state), but reads the program's state
    /// at the block identified by its hash.
    pub async fn read_state_at<D: Decode>(
        &self,
        program_id: ProgramId,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self.read_state_bytes_at(program_id, at).await?;
        D::decode(&mut bytes.as_ref()).map_err(Into::into)
    }

    /// Read the program's state as a byte vector using a meta Wasm.
    pub async fn read_state_bytes_using_wasm(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        argument: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        self.read_state_bytes_using_wasm_at(program_id, fn_name, wasm, argument, None)
            .await
    }

    /// Same as [`read_state_bytes_using_wasm`](Self::read_state_bytes_using_wasm), but reads the program's state at the block identified by its hash.
    pub async fn read_state_bytes_using_wasm_at(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        argument: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        let response: String = self
            .0
            .api()
            .read_state_using_wasm(H256(program_id.into()), fn_name, wasm, argument, at)
            .await?;
        crate::utils::hex_to_vec(response).map_err(Into::into)
    }

    /// Read the program's state as decoded data using a meta Wasm.
    pub async fn read_state_using_wasm<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        argument: Option<E>,
    ) -> Result<D> {
        self.read_state_using_wasm_at(program_id, fn_name, wasm, argument, None)
            .await
    }

    /// Same as [`read_state_using_wasm`](Self::read_state_using_wasm), but
    /// reads the program's state at the block identified by its hash.
    pub async fn read_state_using_wasm_at<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        argument: Option<E>,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self
            .read_state_bytes_using_wasm_at(
                program_id,
                fn_name,
                wasm,
                argument.map(|v| v.encode()),
                at,
            )
            .await?;

        D::decode(&mut bytes.as_ref()).map_err(Into::into)
    }

    /// Read the program's state using a meta Wasm file referenced by its
    /// `path`.
    pub async fn read_state_bytes_using_wasm_by_path(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        path: impl AsRef<Path>,
        argument: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        self.read_state_bytes_using_wasm_by_path_at(program_id, fn_name, path, argument, None)
            .await
    }

    /// Same as [`read_state_using_wasm_by_path`](Self::read_state_using_wasm_by_path), but reads the program's state at the block identified by its hash.
    pub async fn read_state_bytes_using_wasm_by_path_at(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        path: impl AsRef<Path>,
        argument: Option<Vec<u8>>,
        at: Option<H256>,
    ) -> Result<Vec<u8>> {
        self.read_state_bytes_using_wasm_at(
            program_id,
            fn_name,
            utils::code_from_os(path.as_ref())?,
            argument,
            at,
        )
        .await
    }

    /// Read the program's state using a meta Wasm file referenced by its
    /// `path`.
    pub async fn read_state_using_wasm_by_path<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        path: impl AsRef<Path>,
        argument: Option<E>,
    ) -> Result<D> {
        self.read_state_using_wasm_by_path_at(program_id, fn_name, path, argument, None)
            .await
    }

    /// Same as [`read_state_using_wasm_by_path`](Self::read_state_using_wasm_by_path), but reads the program's state at the block identified by its hash.
    pub async fn read_state_using_wasm_by_path_at<E: Encode, D: Decode>(
        &self,
        program_id: ProgramId,
        fn_name: &str,
        path: impl AsRef<Path>,
        argument: Option<E>,
        at: Option<H256>,
    ) -> Result<D> {
        let bytes = self
            .read_state_bytes_using_wasm_by_path_at(
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
    pub async fn read_metahash(&self, program_id: ProgramId) -> Result<H256> {
        self.read_metahash_at(program_id, None).await
    }

    /// Same as [`read_metahash`](Self::read_metahash), but read the program's
    /// metahash at the block identified by its hash.
    pub async fn read_metahash_at(&self, program_id: ProgramId, at: Option<H256>) -> Result<H256> {
        self.0
            .api()
            .read_meta_hash(H256(program_id.into()), at)
            .await
            .map_err(Into::into)
    }

    // Reserved for development usages.
    //
    // NOTE: Please gather the low-level rpc requests in `[gsdk::rpc]` module.
    #[cfg(test)]
    #[allow(unused)]
    async fn rpc_request<T: gsdk::ext::sp_runtime::DeserializeOwned>(
        &self,
        method: &str,
        params: subxt::rpc::RpcParams,
    ) -> Result<T> {
        self.0
            .api()
            .rpc()
            .request(method, params)
            .await
            .map_err(Into::into)
    }
}
