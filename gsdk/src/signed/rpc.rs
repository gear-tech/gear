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

//! RPC calls with signer

use crate::{GasInfo, SignedApi, result::Result};
use gear_core::{
    ids::{ActorId, CodeId, MessageId},
    rpc::ReplyInfo,
};
use gsdk_codegen::at_block;
use subxt::utils::H256;

impl SignedApi {
    /// Returns the signer's free balance.
    pub async fn free_balance(&self) -> Result<u128> {
        self.unsigned().free_balance(self.account_id()).await
    }

    /// Get self reserved balance.
    pub async fn reserved_balance(&self) -> Result<u128> {
        self.unsigned().reserved_balance(self.account_id()).await
    }

    /// Get self total balance.
    pub async fn total_balance(&self) -> Result<u128> {
        self.unsigned().total_balance(self.account_id()).await
    }

    /// Calls `gear_calculateInitCreateGas` RPC method at specified block.
    #[at_block]
    pub async fn calculate_create_gas_at(
        &self,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.unsigned()
            .calculate_create_gas_at(
                self.account_id(),
                code_id,
                payload,
                value,
                allow_other_panics,
                block_hash,
            )
            .await
    }

    /// Calls `gear_calculateInitUploadGas` RPC method at specified block.
    #[at_block]
    pub async fn calculate_upload_gas_at(
        &self,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.unsigned()
            .calculate_upload_gas_at(
                self.account_id(),
                code,
                payload,
                value,
                allow_other_panics,
                block_hash,
            )
            .await
    }

    /// Calls `gear_calculateHandleGas` RPC method at specified block at specified block.
    #[at_block]
    pub async fn calculate_handle_gas_at(
        &self,
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.unsigned()
            .calculate_handle_gas_at(
                self.account_id(),
                destination,
                payload,
                value,
                allow_other_panics,
                block_hash,
            )
            .await
    }

    /// Calls `gear_calculateReplyGas` RPC method at specified block.
    #[at_block]
    pub async fn calculate_reply_gas_at(
        &self,
        message_id: MessageId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        block_hash: Option<H256>,
    ) -> Result<GasInfo> {
        self.unsigned()
            .calculate_reply_gas_at(
                self.account_id(),
                message_id,
                payload,
                value,
                allow_other_panics,
                block_hash,
            )
            .await
    }

    /// Calls `gear_calculateReplyForHandle` RPC method at specified block.
    #[at_block]
    pub async fn calculate_reply_for_handle_at(
        &self,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        block_hash: Option<H256>,
    ) -> Result<ReplyInfo> {
        self.unsigned()
            .calculate_reply_for_handle_at(
                self.account_id(),
                destination,
                payload,
                gas_limit,
                value,
                block_hash,
            )
            .await
    }
}
