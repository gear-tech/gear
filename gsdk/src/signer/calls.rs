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
    IntoSubxt, Result, TxInBlock,
    gear::{
        self,
        runtime_types::{
            pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
            sp_weights::weight_v2::Weight,
            vara_runtime::RuntimeCall,
        },
    },
    signer::utils::EventsResult,
};
use gear_core::ids::*;
use sp_runtime::AccountId32;

/// Implementation of calls to programs/other users for [`Signer`].
#[derive(Clone)]
pub struct SignerCalls<'a>(pub(crate) &'a Inner);

// pallet-balances
impl SignerCalls<'_> {
    /// `pallet_balances::transfer_keep_alive`
    pub async fn transfer_keep_alive(
        &self,
        dest: impl Into<AccountId32>,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_keep_alive(dest.into().into_subxt().into(), value),
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
                gear::tx()
                    .balances()
                    .transfer_allow_death(dest.into().into_subxt().into(), value),
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
                gear::tx()
                    .balances()
                    .transfer_all(dest.into().into_subxt().into(), keep_alive),
            )
            .await
    }
}

// pallet-gear
impl SignerCalls<'_> {
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
                gear::tx()
                    .gear()
                    .create_program(code_id, salt, payload, gas_limit, value, false),
            )
            .await
    }

    /// `pallet_gear::claim_value`
    pub async fn claim_value(&self, message_id: MessageId) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear().claim_value(message_id))
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
                gear::tx()
                    .gear()
                    .send_message(destination, payload, gas_limit, value, false),
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
                gear::tx()
                    .gear()
                    .send_reply(reply_to_id, payload, gas_limit, value, false),
            )
            .await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: Vec<u8>) -> Result<TxInBlock> {
        self.0.run_tx(gear::tx().gear().upload_code(code)).await
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
                gear::tx()
                    .gear()
                    .upload_program(code, salt, payload, gas_limit, value, false),
            )
            .await
    }
}

// pallet-gear-eth-bridge
impl SignerCalls<'_> {
    /// `pallet-gear-eth-bridge::reset_overflowed_queue`
    pub async fn reset_overflowed_queue(
        &self,
        encoded_finality_proof: Vec<u8>,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .gear_eth_bridge()
                    .reset_overflowed_queue(encoded_finality_proof),
            )
            .await
    }
}

// pallet-utility
impl SignerCalls<'_> {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxInBlock> {
        self.0.run_tx(gear::tx().utility().force_batch(calls)).await
    }
}

// pallet-sudo
impl SignerCalls<'_> {
    /// `pallet_sudo::sudo_unchecked_weight`
    pub async fn sudo_unchecked_weight(&self, call: RuntimeCall, weight: Weight) -> EventsResult {
        self.0
            .sudo_run_tx(gear::tx().sudo().sudo_unchecked_weight(call, weight))
            .await
    }
}

// pallet-gear-voucher
impl SignerCalls<'_> {
    /// `pallet_gear_voucher::issue`
    pub async fn issue_voucher(
        &self,
        spender: impl Into<AccountId32>,
        balance: u128,
        programs: Option<Vec<ActorId>>,
        code_uploading: bool,
        duration: u32,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear_voucher().issue(
                spender.into().into_subxt(),
                balance,
                programs,
                code_uploading,
                duration,
            ))
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
        self.0
            .run_tx(gear::tx().gear_voucher().update(
                spender.into().into_subxt(),
                voucher_id,
                move_ownership.map(|id| id.into().into_subxt()),
                balance_top_up,
                append_programs,
                code_uploading,
                Some(prolong_duration),
            ))
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
                gear::tx()
                    .gear_voucher()
                    .revoke(spender.into().into_subxt(), voucher_id),
            )
            .await
    }

    /// `pallet_gear_voucher::decline`
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear_voucher().decline(voucher_id))
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
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
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
            destination,
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
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
            reply_to_id,
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn decline_voucher_with_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::DeclineVoucher;

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }
}
