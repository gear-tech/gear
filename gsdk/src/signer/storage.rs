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

//! Storage interfaces
use crate::{
    Api, BlockNumber, Error, GearGasNode, GearGasNodeId, GearPages,
    metadata::{
        runtime_types::{
            frame_system::pallet::Call,
            gear_core::{
                code::{instrumented::InstrumentedCode, metadata::CodeMetadata},
                program::{ActiveProgram, Program},
            },
            pallet_gear_bank::pallet::BankAccount,
        },
        storage::{GearBankStorage, GearGasStorage, GearProgramStorage},
        vara_runtime::RuntimeCall,
    },
    signer::{Inner, utils::EventsResult},
    utils::storage_address_bytes,
};
use gear_core::{
    ids::*,
    memory::{PageBuf, PageBufInner},
};
use hex::ToHex;
use parity_scale_codec::Encode;
use sp_runtime::AccountId32;
use std::sync::Arc;
use subxt::{
    dynamic::Value,
    metadata::EncodeWithMetadata,
    storage::{Address, StaticStorageKey},
    utils::Static,
};

/// Implementation of storage calls for [`Signer`].
#[derive(Clone)]
pub struct SignerStorage(pub(crate) Arc<Inner>);

// pallet-system
impl SignerStorage {
    /// Sets storage values via calling sudo pallet
    pub async fn set_storage(&self, items: &[(impl Address, impl Encode)]) -> EventsResult {
        let metadata = self.0.api().metadata();
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

        self.0
            .sudo(RuntimeCall::System(Call::set_storage {
                items: items_to_set,
            }))
            .await
    }
}

// pallet-gas
impl SignerStorage {
    /// Writes gas total issuance into storage.
    pub async fn set_total_issuance(&self, value: u64) -> EventsResult {
        let addr = Api::storage_root(GearGasStorage::TotalIssuance);
        self.set_storage(&[(addr, value)]).await
    }

    /// Writes Gear gas nodes into storage at their ids.
    pub async fn set_gas_nodes(
        &self,
        gas_nodes: &impl AsRef<[(GearGasNodeId, GearGasNode)]>,
    ) -> EventsResult {
        let gas_nodes = gas_nodes.as_ref();
        let mut gas_nodes_to_set = Vec::with_capacity(gas_nodes.len());
        for gas_node in gas_nodes {
            let addr = Api::storage(GearGasStorage::GasNodes, StaticStorageKey::new(gas_node.0));
            gas_nodes_to_set.push((addr, &gas_node.1));
        }
        self.set_storage(&gas_nodes_to_set).await
    }
}

// pallet-gear-bank
impl SignerStorage {
    /// Writes given BankAccount info into storage at `AccountId32`.
    pub async fn set_bank_account_storage(
        &self,
        dest: impl Into<AccountId32>,
        value: BankAccount<u128>,
    ) -> EventsResult {
        let addr = Api::storage(GearBankStorage::Bank, vec![Value::from_bytes(dest.into())]);
        self.set_storage(&[(addr, value)]).await
    }
}

// pallet-gear-program
impl SignerStorage {
    /// Writes `InstrumentedCode` into storage at `CodeId`
    pub async fn set_instrumented_code_storage(
        &self,
        code_id: CodeId,
        code: &InstrumentedCode,
    ) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::InstrumentedCodeStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code)]).await
    }

    /// Writes `CodeMetadata` into storage at `CodeId`
    pub async fn set_code_metadata_storage(
        &self,
        code_id: CodeId,
        code_metadata: &CodeMetadata,
    ) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::CodeMetadataStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.set_storage(&[(addr, code_metadata)]).await
    }

    /// Writes `GearPages` into storage at `program_id`
    pub async fn set_gpages(
        &self,
        program_id: ActorId,
        memory_infix: u32,
        program_pages: &GearPages,
    ) -> EventsResult {
        let mut program_pages_to_set = Vec::with_capacity(program_pages.len());
        for program_page in program_pages {
            let addr = Api::storage(
                GearProgramStorage::MemoryPages,
                vec![
                    subxt::dynamic::Value::from_bytes(program_id),
                    subxt::dynamic::Value::u128(memory_infix as u128),
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
        program_id: ActorId,
        program: ActiveProgram<BlockNumber>,
    ) -> EventsResult {
        let addr = Api::storage(
            GearProgramStorage::ProgramStorage,
            vec![Value::from_bytes(program_id)],
        );
        self.set_storage(&[(addr, &Program::Active(program))]).await
    }
}
