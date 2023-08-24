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

//! gear api calls
use super::SignerInner;
use crate::{
    config::GearConfig,
    metadata::{
        calls::{BalancesCall, GearCall, SudoCall, UtilityCall},
        gear_runtime::RuntimeCall,
        runtime_types::sp_weights::weight_v2::Weight,
        sudo::Event as SudoEvent,
        Event,
    },
    signer::SignerRpc,
    Error, Result, TxInBlock, TxStatus,
};
use anyhow::anyhow;
use gear_core::ids::*;
use sp_runtime::AccountId32;
use std::sync::Arc;
use subxt::{
    blocks::ExtrinsicEvents,
    dynamic::Value,
    tx::{DynamicPayload, TxProgress},
    Error as SubxtError, OnlineClient,
};

type TxProgressT = TxProgress<GearConfig, OnlineClient<GearConfig>>;
type EventsResult = Result<ExtrinsicEvents<GearConfig>, Error>;

/// Implementation of calls to programs/other users for [`Signer`].
#[derive(Clone)]
pub struct SignerCalls(pub(crate) Arc<SignerInner>);

// pallet-balances
impl SignerCalls {
    /// `pallet_balances::transfer`
    pub async fn transfer(&self, dest: impl Into<AccountId32>, value: u128) -> Result<TxInBlock> {
        self.0
            .run_tx(
                BalancesCall::Transfer,
                vec![
                    Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
                    Value::u128(value),
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
        prepaid: bool,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::SendMessage,
                vec![
                    Value::from_bytes(destination),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(prepaid),
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
        prepaid: bool,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                GearCall::SendReply,
                vec![
                    Value::from_bytes(reply_to_id),
                    Value::from_bytes(payload),
                    Value::u128(gas_limit as u128),
                    Value::u128(value),
                    Value::bool(prepaid),
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
                ],
            )
            .await
    }
}

// pallet-utility
impl SignerInner {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxInBlock> {
        self.run_tx(
            UtilityCall::ForceBatch,
            vec![calls.into_iter().map(Value::from).collect::<Vec<Value>>()],
        )
        .await
    }
}

// pallet-sudo
impl SignerInner {
    pub async fn process_sudo(&self, tx: DynamicPayload) -> EventsResult {
        let tx = self.process(tx).await?;
        let events = tx.wait_for_success().await?;
        for event in events.iter() {
            let event = event?.as_root_event::<Event>()?;
            if let Event::Sudo(SudoEvent::Sudid {
                sudo_result: Err(err),
            }) = event
            {
                return Err(self.api().decode_error(err).into());
            }
        }

        Ok(events)
    }

    /// `pallet_sudo::sudo_unchecked_weight`
    pub async fn sudo_unchecked_weight(&self, call: RuntimeCall, weight: Weight) -> EventsResult {
        self.sudo_run_tx(
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

    /// `pallet_sudo::sudo`
    pub async fn sudo(&self, call: RuntimeCall) -> EventsResult {
        self.sudo_run_tx(SudoCall::Sudo, vec![Value::from(call)])
            .await
    }
}

// Singer utils
impl SignerInner {
    /// Propagates log::info for given status.
    pub(crate) fn log_status(&self, status: &TxStatus) {
        match status {
            TxStatus::Future => log::info!("	Status: Future"),
            TxStatus::Ready => log::info!("	Status: Ready"),
            TxStatus::Broadcast(v) => log::info!("	Status: Broadcast( {v:?} )"),
            TxStatus::InBlock(b) => log::info!(
                "	Status: InBlock( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Retracted(h) => log::warn!("	Status: Retracted( {h} )"),
            TxStatus::FinalityTimeout(h) => log::error!("	Status: FinalityTimeout( {h} )"),
            TxStatus::Finalized(b) => log::info!(
                "	Status: Finalized( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Usurped(h) => log::error!("	Status: Usurped( {h} )"),
            TxStatus::Dropped => log::error!("	Status: Dropped"),
            TxStatus::Invalid => log::error!("	Status: Invalid"),
        }
    }

    /// Wrapper for submit and watch with nonce.
    async fn sign_and_submit_then_watch<'a>(
        &self,
        tx: &DynamicPayload,
    ) -> Result<TxProgressT, SubxtError> {
        if let Some(nonce) = self.nonce {
            self.api
                .tx()
                .create_signed_with_nonce(tx, &self.signer, nonce, Default::default())?
                .submit_and_watch()
                .await
        } else {
            self.api
                .tx()
                .sign_and_submit_then_watch_default(tx, &self.signer)
                .await
        }
    }

    /// Listen transaction process and print logs.
    pub async fn process<'a>(&self, tx: DynamicPayload) -> Result<TxInBlock> {
        use subxt::tx::TxStatus::*;

        let signer_rpc = SignerRpc(Arc::new(self.clone()));
        let before = signer_rpc.get_balance().await?;

        let mut process = self.sign_and_submit_then_watch(&tx).await?;
        let (pallet, name) = (tx.pallet_name(), tx.call_name());

        log::info!("Submitted extrinsic {}::{}", pallet, name);

        while let Some(status) = process.next_item().await {
            let status = status?;
            self.log_status(&status);
            match status {
                Future | Ready | Broadcast(_) | InBlock(_) | Retracted(_) => (),
                Finalized(b) => {
                    log::info!(
                        "Successfully submitted call {}::{} {} at {}!",
                        pallet,
                        name,
                        b.extrinsic_hash(),
                        b.block_hash()
                    );
                    self.log_balance_spent(before).await?;
                    return Ok(b);
                }
                _ => {
                    self.log_balance_spent(before).await?;
                    return Err(status.into());
                }
            }
        }

        Err(anyhow!("Transaction wasn't found").into())
    }
}
