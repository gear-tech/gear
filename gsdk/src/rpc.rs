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

//! Gear API RPC methods

use crate::{Api, GasInfo, IntoAccountId32, result::Result, utils};
use gear_core::{
    ids::{CodeId, MessageId},
    rpc::ReplyInfo,
};
use gsdk_codegen::at_block;
use parity_scale_codec::Decode;
use subxt::{ext::subxt_rpcs::rpc_params, utils::H256};

impl Api {
    /// Calculates the gas required to create a program from a
    /// code and process an initialization message at specified block.
    ///
    /// Actually call `gear_calculateInitCreateGas` RPC method. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `code_id` is the uploaded code identifier that can be obtained by
    ///   calling the [`SignedApi::upload_code`] function;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    ///
    /// [`SignedApi::upload_code`]: crate::SignedApi::upload_code
    #[at_block]
    pub async fn calculate_create_gas_at(
        &self,
        origin: impl IntoAccountId32,
        code_id: CodeId,
        payload: impl AsRef<[u8]>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitCreateGas",
                rpc_params![
                    H256(origin.into_account_id().0),
                    H256(code_id.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    block_hash
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Calculates the gas required to upload a program and
    /// process an initialization message at specified block.
    ///
    /// Actually calls `gear_calculateInitUploadGas` RPC method. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `code` is the buffer containing the Wasm binary code of the Gear
    ///   program;
    /// - `payload` vector contains data to be processed by the program;
    /// - `value` to be transferred to the program's account;
    /// - `allow_other_panics` flag indicates ignoring a trap during the
    ///   program's execution;
    #[at_block]
    pub async fn calculate_upload_gas_at(
        &self,
        origin: impl IntoAccountId32,
        code: impl AsRef<[u8]>,
        payload: impl AsRef<[u8]>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitUploadGas",
                rpc_params![
                    H256(origin.into_account_id().0),
                    hex::encode(code),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    block_hash
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Calculates the gas required to handle a message at specified block.
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
    #[at_block]
    pub async fn calculate_handle_gas_at(
        &self,
        origin: impl IntoAccountId32,
        destination: impl IntoAccountId32,
        payload: impl AsRef<[u8]>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateHandleGas",
                rpc_params![
                    H256(origin.into_account_id().0),
                    H256(destination.into_account_id().0),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    block_hash
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Calculates the gas required to reply to the received
    /// message from the mailbox at specified block.
    ///
    /// Actually calls `gear_calculateReplyGas` RPC method. The
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
    #[at_block]
    pub async fn calculate_reply_gas_at(
        &self,
        origin: impl IntoAccountId32,
        message_id: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateReplyGas",
                rpc_params![
                    H256(origin.into_account_id().0),
                    H256(message_id.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    block_hash
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// Reads the program's metahash at specified block.
    ///
    /// Actually calls `gear_readMetahash` RPC method.
    #[at_block]
    pub async fn read_metahash_at(
        &self,
        program_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<H256> {
        self.rpc()
            .request(
                "gear_readMetahash",
                rpc_params![H256(program_id.into_account_id().0), block_hash],
            )
            .await
            .map_err(Into::into)
    }

    /// Reads the program's state as a byte vector at specified block.
    ///
    /// Actually sends the `gear_readState` RPC to the node.
    #[at_block]
    pub async fn read_state_bytes_at(
        &self,
        program_id: impl IntoAccountId32,
        payload: impl AsRef<[u8]>,
        block_hash: Option<H256>,
    ) -> Result<Vec<u8>> {
        let response: String = self
            .rpc()
            .request(
                "gear_readState",
                rpc_params![
                    H256(program_id.into_account_id().0),
                    hex::encode(payload),
                    block_hash
                ],
            )
            .await?;

        utils::hex_to_vec(response)
    }

    /// Reads the programs's state as a decoded value at specified block.
    ///
    /// See [`Self::read_state_bytes_at`] for details.
    #[at_block]
    pub async fn read_state_at<T: Decode>(
        &self,
        program_id: impl IntoAccountId32,
        payload: impl AsRef<[u8]>,
        block_hash: Option<H256>,
    ) -> Result<T> {
        let bytes = self
            .read_state_bytes_at(program_id, payload, block_hash)
            .await?;

        Ok(T::decode(&mut bytes.as_slice())?)
    }

    /// Calls `runtime_wasmBlobVersion` RPC method at specified block.
    #[at_block]
    pub async fn runtime_wasm_blob_version_at(&self, block_hash: Option<H256>) -> Result<String> {
        self.rpc()
            .request("runtime_wasmBlobVersion", rpc_params![block_hash])
            .await
            .map_err(Into::into)
    }

    /// Calculates a reply to a given message at specified block.
    ///
    /// Actually calls `gear_calculateReplyForHandle` RPC method. The
    /// function's parameters are:
    ///
    /// - `origin` (optional) is the caller's public address;
    /// - `destination` is the program address;
    /// - `payload` vector contains data to be processed by the program;
    /// - `gas_limit`: maximum amount of gas the program can spend before it is
    ///   halted.
    /// - `value` to be transferred to the program's account;
    #[at_block]
    pub async fn calculate_reply_for_handle_at(
        &self,
        origin: impl IntoAccountId32,
        destination: impl IntoAccountId32,
        payload: impl AsRef<[u8]>,
        gas_limit: u64,
        value: u128,
        block_hash: Option<H256>,
    ) -> Result<ReplyInfo> {
        self.rpc()
            .request(
                "gear_calculateReplyForHandle",
                rpc_params![
                    H256(origin.into_account_id().0),
                    H256(destination.into_account_id().0),
                    hex::encode(payload),
                    gas_limit,
                    value,
                    block_hash
                ],
            )
            .await
            .map_err(Into::into)
    }
}
