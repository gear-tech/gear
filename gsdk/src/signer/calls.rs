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
use crate::{
    config::GearConfig,
    metadata::{
        calls::{BalancesCall, GearCall, SudoCall, UtilityCall},
        gear_runtime::RuntimeCall,
        runtime_types::{
            frame_system::pallet::Call,
            gear_common::{ActiveProgram, Program},
            gear_core::code::InstrumentedCode,
            sp_weights::weight_v2::Weight,
        },
        storage::{GearGasStorage, GearProgramStorage},
        sudo::Event as SudoEvent,
        Event,
    },
    signer::Signer,
    types::{self, InBlock, TxStatus},
    utils::storage_address_bytes,
    Api, BlockNumber, Error,
};
use anyhow::anyhow;
use gear_core::{
    ids::*,
    memory::{PageBuf, PageBufInner},
};
use hex::ToHex;
use parity_scale_codec::Encode;
use sp_runtime::AccountId32;
use subxt::{
    blocks::ExtrinsicEvents,
    dynamic::Value,
    metadata::EncodeWithMetadata,
    storage::StorageAddress,
    tx::{DynamicPayload, TxProgress},
    utils::Static,
    Error as SubxtError, OnlineClient,
};

type TxProgressT = TxProgress<GearConfig, OnlineClient<GearConfig>>;
type EventsResult = Result<ExtrinsicEvents<GearConfig>, Error>;

// pallet-balances
impl Signer {
    /// `pallet_balances::transfer`
    pub async fn transfer(&self, dest: impl Into<AccountId32>, value: u128) -> InBlock {
        self.run_tx(
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
impl Signer {
    /// `pallet_gear::create_program`
    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: Vec<u8>,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> InBlock {
        self.run_tx(
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
    pub async fn claim_value(&self, message_id: MessageId) -> InBlock {
        self.run_tx(GearCall::ClaimValue, vec![Value::from_bytes(message_id)])
            .await
    }

    /// `pallet_gear::send_message`
    pub async fn send_message(
        &self,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> InBlock {
        self.run_tx(
            GearCall::SendMessage,
            vec![
                Value::from_bytes(destination),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
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
    ) -> InBlock {
        self.run_tx(
            GearCall::SendReply,
            vec![
                Value::from_bytes(reply_to_id),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
            ],
        )
        .await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: Vec<u8>) -> InBlock {
        self.run_tx(GearCall::UploadCode, vec![Value::from_bytes(code)])
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
    ) -> InBlock {
        self.run_tx(
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
impl Signer {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> InBlock {
        self.run_tx(
            UtilityCall::ForceBatch,
            vec![calls.into_iter().map(Value::from).collect::<Vec<Value>>()],
        )
        .await
    }
}

// pallet-sudo
impl Signer {
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

// pallet-system
impl Signer {
    /// Sets storage values via calling sudo pallet
    pub async fn set_storage(&self, items: &[(impl StorageAddress, impl Encode)]) -> EventsResult {
        let metadata = self.api().metadata();
        let mut items_to_set = Vec::with_capacity(items.len());
        for item in items {
            let item_key = storage_address_bytes(&item.0, &metadata)?;
            let mut item_value_bytes = Vec::new();
            let item_value_type_id = crate::storage::storage_type_id(&metadata, &item.0)?;
            Static(&item.1).encode_with_metadata(
                item_value_type_id,
                &metadata,
                &mut item_value_bytes,
            )?;
            items_to_set.push((item_key, item_value_bytes));
        }

        self.sudo(RuntimeCall::System(Call::set_storage {
            items: items_to_set,
        }))
        .await
    }
}

// pallet-gas
impl Signer {
    /// Writes gas total issuance into storage.
    pub async fn set_total_issuance(&self, value: u64) -> EventsResult {
        let addr = Api::storage_root(GearGasStorage::TotalIssuance);
        self.set_storage(&[(addr, value)]).await
    }

    /// Writes Gear gas nodes into storage at their ids.
    pub async fn set_gas_nodes(
        &self,
        gas_nodes: &impl AsRef<[(types::GearGasNodeId, types::GearGasNode)]>,
    ) -> EventsResult {
        let gas_nodes = gas_nodes.as_ref();
        let mut gas_nodes_to_set = Vec::with_capacity(gas_nodes.len());
        for gas_node in gas_nodes {
            let addr = Api::storage(GearGasStorage::GasNodes, vec![Static(gas_node.0)]);
            gas_nodes_to_set.push((addr, &gas_node.1));
        }
        self.set_storage(&gas_nodes_to_set).await
    }
}

// pallet-gear-program
impl Signer {
    /// Writes `InstrumentedCode` length into storage at `CodeId`
    pub async fn set_code_len_storage(&self, code_id: CodeId, code_len: u32) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::CodeLenStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code_len)]).await
    }

    /// Writes `InstrumentedCode` into storage at `CodeId`
    pub async fn set_code_storage(&self, code_id: CodeId, code: &InstrumentedCode) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::CodeStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code)]).await
    }

    /// Writes `GearPages` into storage at `program_id`
    pub async fn set_gpages(
        &self,
        program_id: ProgramId,
        program_pages: &types::GearPages,
    ) -> EventsResult {
        let mut program_pages_to_set = Vec::with_capacity(program_pages.len());
        for program_page in program_pages {
            let addr = Api::storage(
                GearProgramStorage::MemoryPageStorage,
                vec![
                    subxt::dynamic::Value::from_bytes(program_id),
                    subxt::dynamic::Value::u128(*program_page.0 as u128),
                ],
            );
            let page_buf_inner = PageBufInner::try_from(program_page.1.clone())
                .map_err(|_| Error::PageInvalid(*program_page.0, program_id.encode_hex()))?;
            let value = PageBuf::from_inner(page_buf_inner);
            program_pages_to_set.push((addr, value));
        }
        self.set_storage(&program_pages_to_set).await
    }

    /// Writes `ActiveProgram` into storage at `program_id`
    pub async fn set_gprog(
        &self,
        program_id: ProgramId,
        program: ActiveProgram<BlockNumber>,
    ) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::ProgramStorage,
            vec![Value::from_bytes(program_id)],
        );
        self.set_storage(&[(addr, &Program::Active(program))]).await
    }
}

// Singer utils
impl Signer {
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
    pub async fn process<'a>(&self, tx: DynamicPayload) -> InBlock {
        use subxt::tx::TxStatus::*;

        let before = self.balance().await?;
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
