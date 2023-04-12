// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    metadata::runtime_types::{
        frame_system::pallet::Call,
        gear_common::{ActiveProgram, Program},
        gear_core::code::InstrumentedCode,
        gear_runtime::RuntimeCall,
        sp_weights::weight_v2::Weight,
    },
    signer::Signer,
    types::{self, InBlock, TxStatus},
    Error,
};
use anyhow::anyhow;
use async_recursion::async_recursion;
use gear_core::{
    ids::*,
    memory::{PageBuf, PageBufInner},
};
use hex::ToHex;
use parity_scale_codec::Encode;
use sp_runtime::AccountId32;
use subxt::{
    dynamic::Value,
    metadata::EncodeWithMetadata,
    storage::StorageAddress,
    tx::{DynamicTxPayload, TxProgress},
    Error as SubxtError, OnlineClient,
};

type TxProgressT = TxProgress<GearConfig, OnlineClient<GearConfig>>;

const ERRORS_REQUIRE_RETRYING: [&str; 2] = ["Connection reset by peer", "Connection refused"];

// pallet-balances
impl Signer {
    /// `pallet_balances::transfer`
    pub async fn transfer(&self, dest: impl Into<AccountId32>, value: u128) -> InBlock {
        let tx = subxt::dynamic::tx(
            "Balances",
            "transfer",
            vec![
                Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
                Value::u128(value),
            ],
        );

        self.process(tx).await
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
        let tx = subxt::dynamic::tx(
            "Gear",
            "create_program",
            vec![
                Value::from_bytes(code_id),
                Value::from_bytes(salt),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
            ],
        );

        self.process(tx).await
    }

    /// `pallet_gear::claim_value`
    pub async fn claim_value(&self, message_id: MessageId) -> InBlock {
        let tx = subxt::dynamic::tx("Gear", "claim_value", vec![Value::from_bytes(message_id)]);
        self.process(tx).await
    }

    /// `pallet_gear::send_message`
    pub async fn send_message(
        &self,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> InBlock {
        let tx = subxt::dynamic::tx(
            "Gear",
            "send_message",
            vec![
                Value::from_bytes(destination),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
            ],
        );

        self.process(tx).await
    }

    /// `pallet_gear::send_reply`
    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> InBlock {
        let tx = subxt::dynamic::tx(
            "Gear",
            "send_reply",
            vec![
                Value::from_bytes(reply_to_id),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
            ],
        );

        self.process(tx).await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: Vec<u8>) -> InBlock {
        let tx = subxt::dynamic::tx("Gear", "upload_code", vec![Value::from_bytes(code)]);
        self.process(tx).await
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
        let tx = subxt::dynamic::tx(
            "Gear",
            "upload_program",
            vec![
                Value::from_bytes(code),
                Value::from_bytes(salt),
                Value::from_bytes(payload),
                Value::u128(gas_limit as u128),
                Value::u128(value),
            ],
        );

        self.process(tx).await
    }
}

// pallet-utility
impl Signer {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> InBlock {
        let tx = subxt::dynamic::tx(
            "Utility",
            "force_batch",
            vec![calls.into_iter().map(Value::from).collect::<Vec<Value>>()],
        );

        self.process(tx).await
    }
}

// pallet-sudo
impl Signer {
    /// `pallet_sudo::sudo_unchecked_weight`
    pub async fn sudo_unchecked_weight(&self, call: RuntimeCall, weight: Weight) -> InBlock {
        let tx = subxt::dynamic::tx(
            "Sudo",
            "sudo_unchecked_weight",
            // As `call` implements conversion to `Value`.
            vec![
                call.into(),
                Value::named_composite([
                    ("ref_time", Value::u128(weight.ref_time as u128)),
                    ("proof_size", Value::u128(weight.proof_size as u128)),
                ]),
            ],
        );

        self.process(tx).await
    }

    /// `pallet_sudo::sudo`
    pub async fn sudo(&self, call: RuntimeCall) -> InBlock {
        let tx = subxt::dynamic::tx("Sudo", "sudo", vec![Value::from(call)]);

        self.process(tx).await
    }
}

// pallet-system
impl Signer {
    /// Sets storage values via calling sudo pallet
    pub async fn set_storage(&self, items: &[(impl StorageAddress, impl Encode)]) -> InBlock {
        let metadata = self.api().metadata();
        let mut items_to_set = Vec::with_capacity(items.len());
        for item in items {
            let item_key = subxt::storage::utils::storage_address_bytes(&item.0, &metadata)?;
            let mut item_value_bytes = Vec::new();
            let item_value_type_id = crate::storage::storage_type_id(&metadata, &item.0)?;
            subxt::metadata::EncodeStaticType(&item.1).encode_with_metadata(
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
    pub async fn set_total_issuance(&self, value: u64) -> InBlock {
        let addr = subxt::dynamic::storage_root("GearGas", "TotalIssuance");
        self.set_storage(&[(addr, value)]).await
    }

    /// Writes Gear gas nodes into storage at their ids.
    pub async fn set_gas_nodes(
        &self,
        gas_nodes: &impl AsRef<[(types::GearGasNodeId, types::GearGasNode)]>,
    ) -> InBlock {
        let gas_nodes = gas_nodes.as_ref();
        let mut gas_nodes_to_set = Vec::with_capacity(gas_nodes.len());
        for gas_node in gas_nodes {
            let addr = subxt::dynamic::storage(
                "GearGas",
                "GasNodes",
                vec![subxt::metadata::EncodeStaticType(gas_node.0)],
            );
            gas_nodes_to_set.push((addr, &gas_node.1));
        }
        self.set_storage(&gas_nodes_to_set).await
    }
}

// pallet-gear-program
impl Signer {
    /// Writes `InstrumentedCode` length into storage at `CodeId`
    pub async fn set_code_len_storage(&self, code_id: CodeId, code_len: u32) -> InBlock {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeLenStorage",
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code_len)]).await
    }

    /// Writes `InstrumentedCode` into storage at `CodeId`
    pub async fn set_code_storage(&self, code_id: CodeId, code: &InstrumentedCode) -> InBlock {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeStorage",
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code)]).await
    }

    /// Writes `GearPages` into storage at `program_id`
    pub async fn set_gpages(
        &self,
        program_id: ProgramId,
        program_pages: &types::GearPages,
    ) -> InBlock {
        let mut program_pages_to_set = Vec::with_capacity(program_pages.len());
        for program_page in program_pages {
            let addr = subxt::dynamic::storage(
                "GearProgram",
                "MemoryPageStorage",
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
        program: ActiveProgram,
        block_number: u32,
    ) -> InBlock {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "ProgramStorage",
            vec![Value::from_bytes(program_id)],
        );
        self.set_storage(&[(addr, &(Program::Active(program), block_number))])
            .await
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
            TxStatus::Retracted(h) => log::info!("	Status: Retracted( {h} )"),
            TxStatus::FinalityTimeout(h) => log::info!("	Status: FinalityTimeout( {h} )"),
            TxStatus::Finalized(b) => log::info!(
                "	Status: Finalized( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Usurped(h) => log::info!("	Status: Usurped( {h} )"),
            TxStatus::Dropped => log::info!("	Status: Dropped"),
            TxStatus::Invalid => log::info!("	Status: Invalid"),
        }
    }

    /// Wrapper for submit and watch with error handling.
    #[async_recursion(?Send)]
    async fn sign_and_submit_then_watch<'a>(
        &self,
        tx: &DynamicTxPayload<'a>,
        counter: u16,
    ) -> Result<TxProgressT, SubxtError> {
        let process = if let Some(nonce) = self.nonce {
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
        };

        if counter >= self.api().retry {
            return process;
        }

        // TODO: Add more patterns for this retrying job.
        if let Err(SubxtError::Rpc(rpc_error)) = &process {
            let error_string = rpc_error.to_string();
            for error in ERRORS_REQUIRE_RETRYING {
                if error_string.contains(error) {
                    return self.sign_and_submit_then_watch(tx, counter + 1).await;
                }
            }
        }

        process
    }

    /// Listen transaction process and print logs.
    async fn process<'a>(&self, tx: DynamicTxPayload<'a>) -> InBlock {
        use subxt::tx::TxStatus::*;

        let before = self.balance().await?;
        let mut process = self.sign_and_submit_then_watch(&tx, 0).await?;

        // Get extrinsic details.
        let (pallet, name) = (tx.pallet_name(), tx.call_name());
        log::info!("Submitted extrinsic {}::{}", pallet, name);

        while let Some(status) = process.next_item().await {
            let status = status?;
            self.log_status(&status);
            match status {
                Future | Ready | Broadcast(_) | InBlock(_) => (),
                Dropped | Invalid | Usurped(_) | FinalityTimeout(_) | Retracted(_) => {
                    self.log_balance_spent(before).await?;
                    return Err(status.into());
                }
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
            }
        }

        Err(anyhow!("Transaction wasn't found").into())
    }
}
