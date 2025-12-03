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

use crate::{GasInfo, result::Result, signer::Inner};
use gear_core::{
    ids::{ActorId, CodeId, MessageId},
    rpc::ReplyInfo,
};
use subxt::utils::H256;

/// Implementation of calls to node RPC for [`Signer`].
#[derive(Clone)]
pub struct SignerRpc<'a>(pub(crate) &'a Inner);

impl SignerRpc<'_> {
    /// public key of the signer in H256
    pub fn source(&self) -> H256 {
        AsRef::<[u8; 32]>::as_ref(self.0.account_id()).into()
    }

    /// Get self free balance.
    pub async fn free_balance(&self) -> Result<u128> {
        self.0.api().free_balance(self.0.account_id()).await
    }

    /// Get self reserved balance.
    pub async fn reserved_balance(&self) -> Result<u128> {
        self.0.api().reserved_balance(self.0.account_id()).await
    }

    /// Get self total balance.
    pub async fn total_balance(&self) -> Result<u128> {
        self.0.api().total_balance(self.0.account_id()).await
    }

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
            .api
            .calculate_create_gas(
                origin.unwrap_or_else(|| self.source()),
                code_id,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
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
            .api
            .calculate_upload_gas(
                origin.unwrap_or_else(|| self.source()),
                code,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
    }

    /// gear_calculateHandleGas
    pub async fn calculate_handle_gas(
        &self,
        origin: Option<H256>,
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .api
            .calculate_handle_gas(
                origin.unwrap_or_else(|| self.source()),
                destination,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
    }

    /// gear_calculateReplyGas
    pub async fn calculate_reply_gas(
        &self,
        origin: Option<H256>,
        message_id: MessageId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.0
            .api
            .calculate_reply_gas(
                origin.unwrap_or_else(|| self.source()),
                message_id,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
    }

    /// gear_calculateReplyForHandle
    pub async fn calculate_reply_for_handle(
        &self,
        origin: Option<H256>,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        at: Option<H256>,
    ) -> Result<ReplyInfo> {
        self.0
            .api
            .calculate_reply_for_handle(
                origin.unwrap_or_else(|| self.source()),
                destination,
                payload,
                gas_limit,
                value,
                at,
            )
            .await
    }
}
