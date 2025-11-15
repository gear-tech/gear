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

//! gear api calls
use super::Inner;
use crate::{
    Result, TxInBlock,
    metadata::{
        Convert,
        calls::{
            BalancesCall, GearCall, GearEthBridgeCall, GearVoucherCall, SudoCall, UtilityCall,
        },
        runtime_types::{
            pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
            sp_weights::weight_v2::Weight,
        },
        vara_runtime::RuntimeCall,
    },
    signer::utils::EventsResult,
};
use gear_core::ids::*;
use sp_runtime::AccountId32;
use std::sync::Arc;
use subxt::dynamic::Value;

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
                BalancesCall::TransferAll,
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
        destination: ActorId,
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

// pallet-gear-eth-bridge
impl SignerCalls {
    /// `pallet-gear-eth-bridge::reset_overflowed_queue`
    pub async fn reset_overflowed_queue(
        &self,
        encoded_finality_proof: Vec<u8>,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearEthBridgeCall::ResetOverflowedQueue,
                vec![Value::from_bytes(encoded_finality_proof)],
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

// pallet-gear-voucher
impl SignerCalls {
    /// `pallet_gear_voucher::issue`
    pub async fn issue_voucher(
        &self,
        spender: impl Into<AccountId32>,
        balance: u128,
        programs: Option<Vec<ActorId>>,
        code_uploading: bool,
        duration: u32,
    ) -> Result<TxInBlock> {
        let programs_value = programs
            .map(|vec| {
                Value::unnamed_composite(vec.into_iter().map(Value::from_bytes).collect::<Vec<_>>())
            })
            .convert();

        self.0
            .run_tx(
                GearVoucherCall::Issue,
                vec![
                    Value::from_bytes(spender.into()),
                    Value::u128(balance),
                    programs_value,
                    Value::bool(code_uploading),
                    Value::from(duration),
                ],
            )
            .await
    }

    /// `pallet_gear_voucher::update`
    #[allow(clippy::too_many_arguments)]
    pub async fn update_voucher(
        &self,
        spender: impl Into<AccountId32>,
        voucher_id: VoucherId,
        move_ownership: Option<impl Into<AccountId32>>,
        balance_top_up: Option<u128>,
        append_programs: Option<Option<Vec<ActorId>>>,
        code_uploading: Option<bool>,
        prolong_duration: u32,
    ) -> Result<TxInBlock> {
        let append_programs_value = append_programs
            .map(|o| {
                o.map(|vec| {
                    Value::unnamed_composite(
                        vec.into_iter().map(Value::from_bytes).collect::<Vec<_>>(),
                    )
                })
                .convert()
            })
            .convert();

        self.0
            .run_tx(
                GearVoucherCall::Update,
                vec![
                    Value::from_bytes(spender.into()),
                    Value::from_bytes(voucher_id.0),
                    move_ownership
                        .map(|v| Value::from_bytes(v.into()))
                        .convert(),
                    balance_top_up.map(Value::u128).convert(),
                    append_programs_value,
                    code_uploading.map(Value::bool).convert(),
                    Value::from(prolong_duration),
                ],
            )
            .await
    }

    /// `pallet_gear_voucher::revoke`
    pub async fn revoke_voucher(
        &self,
        spender: impl Into<AccountId32>,
        voucher_id: VoucherId,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearVoucherCall::Revoke,
                vec![
                    Value::from_bytes(spender.into()),
                    Value::from_bytes(voucher_id.0),
                ],
            )
            .await
    }

    /// `pallet_gear_voucher::decline`
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearVoucherCall::Decline,
                vec![Value::from_bytes(voucher_id.0)],
            )
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn upload_code_with_voucher(
        &self,
        voucher_id: VoucherId,
        code: Vec<u8>,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::UploadCode { code };

        self.0
            .run_tx(
                GearVoucherCall::Call,
                vec![Value::from_bytes(voucher_id.0), call.into()],
            )
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn send_message_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::SendMessage {
            destination: destination.into(),
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(
                GearVoucherCall::Call,
                vec![Value::from_bytes(voucher_id.0), call.into()],
            )
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn send_reply_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::SendReply {
            reply_to_id: reply_to_id.into(),
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(
                GearVoucherCall::Call,
                vec![Value::from_bytes(voucher_id.0), call.into()],
            )
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn decline_voucher_with_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::DeclineVoucher;

        self.0
            .run_tx(
                GearVoucherCall::Call,
                vec![Value::from_bytes(voucher_id.0), call.into()],
            )
            .await
    }
}
