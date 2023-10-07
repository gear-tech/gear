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

//! Gear API RPC methods

use crate::{result::Result, Api, GasInfo};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use sp_core::H256;
use subxt::{ext::codec::Decode, rpc_params};

impl Api {
    /// gear_calculateInitCreateGas
    pub async fn calculate_create_gas(
        &self,
        origin: H256,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        maybe_at: Option<H256>,
    ) -> Result<GasInfo> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![
            origin,
            H256(code_id.into()),
            hex::encode(payload),
            value,
            allow_other_panics,
            maybe_at
        ]
        .build()
        .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_calculateInitCreateGas", params.as_deref(), at)
            .await?;

        GasInfo::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_calculateInitUploadGas
    pub async fn calculate_upload_gas(
        &self,
        origin: H256,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        maybe_at: Option<H256>,
    ) -> Result<GasInfo> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![
            origin,
            hex::encode(code),
            hex::encode(payload),
            value,
            allow_other_panics,
            maybe_at
        ]
        .build()
        .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_calculateInitUploadGas", params.as_deref(), at)
            .await?;

        GasInfo::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_calculateHandleGas
    pub async fn calculate_handle_gas(
        &self,
        origin: H256,
        destination: ProgramId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        maybe_at: Option<H256>,
    ) -> Result<GasInfo> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![
            origin,
            H256(destination.into()),
            hex::encode(payload),
            value,
            allow_other_panics,
            maybe_at
        ]
        .build()
        .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_calculateHandleGas", params.as_deref(), at)
            .await?;

        GasInfo::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_calculateReplyGas
    pub async fn calculate_reply_gas(
        &self,
        origin: H256,
        message_id: MessageId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        maybe_at: Option<H256>,
    ) -> Result<GasInfo> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![
            origin,
            H256(message_id.into()),
            hex::encode(payload),
            value,
            allow_other_panics,
            maybe_at
        ]
        .build()
        .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_calculateReplyGas", params.as_deref(), at)
            .await?;

        GasInfo::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_meta_hash
    pub async fn read_meta_hash(&self, pid: H256, maybe_at: Option<H256>) -> Result<H256> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![H256(pid.into()), maybe_at]
            .build()
            .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_readMetahash", params.as_deref(), at)
            .await?;

        H256::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_readState
    pub async fn read_state(
        &self,
        pid: H256,
        payload: Vec<u8>,
        maybe_at: Option<H256>,
    ) -> Result<String> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![H256(pid.into()), hex::encode(payload), maybe_at]
            .build()
            .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_readState", params.as_deref(), at)
            .await?;
        String::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// gear_readStateUsingWasm
    pub async fn read_state_using_wasm(
        &self,
        pid: H256,
        payload: Vec<u8>,
        method: &str,
        wasm: Vec<u8>,
        args: Option<Vec<u8>>,
        maybe_at: Option<H256>,
    ) -> Result<String> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![
            pid,
            hex::encode(payload),
            hex::encode(method),
            hex::encode(wasm),
            args.map(hex::encode),
            maybe_at
        ]
        .build()
        .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("gear_readStateUsingWasm", params.as_deref(), at)
            .await?;

        String::decode(&mut r.as_ref()).map_err(Into::into)
    }

    /// runtime_wasmBlobVersion
    pub async fn runtime_wasm_blob_version(&self, maybe_at: Option<H256>) -> Result<String> {
        let at = if let Some(at) = maybe_at {
            at
        } else {
            self.backend().latest_finalized_block_ref().await?.hash()
        };

        let params = rpc_params![maybe_at]
            .build()
            .map(|p| p.get().as_bytes().to_vec());

        let r = self
            .backend()
            .call("runtime_wasmBlobVersion", params.as_deref(), at)
            .await?;

        String::decode(&mut r.as_ref()).map_err(Into::into)
    }
}
