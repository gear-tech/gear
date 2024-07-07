// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! gear api calls
use super::Inner;
use crate::{
    config::GearConfig,
    metadata::{
        calls::{BalancesCall, GearCall, SudoCall, UtilityCall},
        runtime_types::sp_weights::weight_v2::Weight,
        vara_runtime::RuntimeCall,
    },
    Error, Result, TxInBlock,
};
use gear_core::ids::*;
use sp_runtime::AccountId32;
use std::sync::Arc;
use subxt::{blocks::ExtrinsicEvents, dynamic::Value};

type EventsResult = Result<ExtrinsicEvents<GearConfig>, Error>;

/// Implementation of calls to programs/other users for [`Signer`].
#[derive(Clone)]
pub struct SignerCalls(pub(crate) Arc<Inner>);

// pallet-balances
impl SignerCalls {
    /// `pallet_balances::transfer_keep_alive`
    pub async fn transfer_keep_alive(
        &self,
        dest: impl Into<AccountId32>,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                BalancesCall::TransferKeepAlive,
                vec![
                    Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
                    Value::u128(value),
                ],
            )
            .await
    }

    /// `pallet_balances::transfer_allow_death`
    pub async fn transfer_allow_death(
        &self,
        dest: impl Into<AccountId32>,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                BalancesCall::TransferAllowDeath,
                vec![
                    Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
                    Value::u128(value),
                ],
            )
            .await
    }

    /// `pallet_balances::transfer_all`
    pub async fn transfer_all(
        &self,
        dest: impl Into<AccountId32>,
        keep_alive: bool,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                BalancesCall::TransferAllowDeath,
                vec![
                    Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
                    Value::bool(keep_alive),
                ],
            )
            .await
    }
}

// pallet-gear
impl SignerCalls {
    /// `pallet_gear::create_program`
    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: Vec<u8>,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::CreateProgram,
                vec![
                    Value::from_bytes(code_id),
                    Value::from_bytes(salt),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(false),
                ],
            )
            .await
    }

    /// `pallet_gear::claim_value`
    pub async fn claim_value(&self, message_id: MessageId) -> Result<TxInBlock> {
        self.0
            .run_tx(GearCall::ClaimValue, vec![Value::from_bytes(message_id)])
            .await
    }

    /// `pallet_gear::send_message`
    pub async fn send_message(
        &self,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::SendMessage,
                vec![
                    Value::from_bytes(destination),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(false),
                ],
            )
            .await
    }

    /// `pallet_gear::send_reply`
    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::SendReply,
                vec![
                    Value::from_bytes(reply_to_id),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(false),
                ],
            )
            .await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: Vec<u8>) -> Result<TxInBlock> {
        self.0
            .run_tx(GearCall::UploadCode, vec![Value::from_bytes(code)])
            .await
    }

    /// `pallet_gear::upload_program`
    pub async fn upload_program(
        &self,
        code: Vec<u8>,
        salt: Vec<u8>,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::UploadProgram,
                vec![
                    Value::from_bytes(code),
                    Value::from_bytes(salt),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(false),
                ],
            )
            .await
    }
}

// pallet-utility
impl SignerCalls {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxInBlock> {
        self.0
            .run_tx(
                UtilityCall::ForceBatch,
                vec![calls.into_iter().map(Value::from).collect::<Vec<Value>>()],
            )
            .await
    }
}

// pallet-sudo
impl SignerCalls {
    /// `pallet_sudo::sudo_unchecked_weight`
    pub async fn sudo_unchecked_weight(&self, call: RuntimeCall, weight: Weight) -> EventsResult {
        self.0
            .sudo_run_tx(
                SudoCall::SudoUncheckedWeight,
                // As `call` implements conversion to `Value`.
                vec![
                    call.into(),
                    Value::named_composite([
                        ("ref_time", Value::u128(weight.ref_time as u128)),
                        ("proof_size", Value::u128(weight.proof_size as u128)),
                    ]),
                ],
            )
            .await
    }
}
