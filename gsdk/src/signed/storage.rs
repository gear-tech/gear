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

use super::SignedApi;
use crate::{
    BlockNumber, GearGasNode, GearGasNodeId, GearPages, IntoAccountId32, Result, TxEvents,
    gear::{
        self,
        runtime_types::{
            frame_system::pallet::Call,
            gear_common::storage::primitives::Interval,
            gear_core::program::{ActiveProgram, Program},
            pallet_gear_bank::pallet::BankAccount,
            vara_runtime::RuntimeCall,
        },
    },
    utils::storage_address_bytes,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId, MessageId},
    message::UserStoredMessage,
    program::MemoryInfix,
};
use subxt::{metadata::EncodeWithMetadata, storage::Address};

// pallet-system
impl SignedApi {
    /// Sets storage values via calling sudo pallet
    pub async fn set_storage(
        &self,
        items: &[(impl Address, impl EncodeWithMetadata)],
    ) -> Result<TxEvents> {
        let metadata = self.unsigned().metadata();
        let mut items_to_set = Vec::with_capacity(items.len());
        for item in items {
            let item_key = storage_address_bytes(&item.0, &metadata)?;
            let mut item_value_bytes = Vec::new();
            let item_value_type_id = crate::storage::storage_type_id(&metadata, &item.0)?;
            item.1
                .encode_with_metadata(item_value_type_id, &metadata, &mut item_value_bytes)?;
            items_to_set.push((item_key, item_value_bytes));
        }

        self.sudo(RuntimeCall::System(Call::set_storage {
            items: items_to_set,
        }))
        .await
    }
}

// pallet-gas
impl SignedApi {
    /// Writes gas total issuance into storage.
    pub async fn set_total_issuance(&self, value: u64) -> Result<TxEvents> {
        self.set_storage(&[(gear::storage().gear_gas().total_issuance(), value)])
            .await
    }

    /// Writes Gear gas nodes into storage at their ids.
    pub async fn set_gas_nodes(
        &self,
        gas_nodes: &impl AsRef<[(GearGasNodeId, GearGasNode)]>,
    ) -> Result<TxEvents> {
        let gas_nodes = gas_nodes.as_ref();
        let mut gas_nodes_to_set = Vec::with_capacity(gas_nodes.len());
        for gas_node in gas_nodes {
            gas_nodes_to_set.push((
                gear::storage().gear_gas().gas_nodes(gas_node.0.clone()),
                &gas_node.1,
            ));
        }
        self.set_storage(&gas_nodes_to_set).await
    }
}

// pallet-gear-bank
impl SignedApi {
    /// Writes given BankAccount info into storage at `dest`.
    pub async fn set_bank_account_storage(
        &self,
        dest: impl IntoAccountId32,
        value: BankAccount<u128>,
    ) -> Result<TxEvents> {
        self.set_storage(&[(
            gear::storage().gear_bank().bank(dest.into_account_id()),
            value,
        )])
        .await
    }
}

// pallet-gear-program
impl SignedApi {
    /// Writes `InstrumentedCode` into storage at `CodeId`
    pub async fn set_instrumented_code_storage(
        &self,
        code_id: CodeId,
        code: &InstrumentedCode,
    ) -> Result<TxEvents> {
        self.set_storage(&[(
            gear::storage()
                .gear_program()
                .instrumented_code_storage(code_id),
            code,
        )])
        .await
    }

    /// Writes `CodeMetadata` into storage at `CodeId`
    pub async fn set_code_metadata_storage(
        &self,
        code_id: CodeId,
        code_metadata: &CodeMetadata,
    ) -> Result<TxEvents> {
        self.set_storage(&[(
            gear::storage()
                .gear_program()
                .code_metadata_storage(code_id),
            code_metadata,
        )])
        .await
    }

    /// Writes `GearPages` into storage at `program_id`
    pub async fn set_program_pages(
        &self,
        program_id: ActorId,
        program_pages: &GearPages,
    ) -> Result<TxEvents> {
        let mut program_pages_to_set = Vec::with_capacity(program_pages.len());
        for (&page_index, value) in program_pages {
            let addr = gear::storage().gear_program().memory_pages(
                program_id,
                MemoryInfix::default(),
                page_index.into(),
            );
            program_pages_to_set.push((addr, value));
        }
        self.set_storage(&program_pages_to_set).await
    }

    /// Writes `ActiveProgram` into storage at `program_id`
    pub async fn set_program(
        &self,
        program_id: ActorId,
        program: ActiveProgram<BlockNumber>,
    ) -> Result<TxEvents> {
        self.set_storage(&[(
            gear::storage().gear_program().program_storage(program_id),
            &Program::Active(program),
        )])
        .await
    }
}

// pallet-gear-messenger
impl SignedApi {
    /// Retrieves a message identified by `message_id` from the mailbox.
    pub async fn mailbox_message(
        &self,
        message_id: MessageId,
    ) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        self.mailbox_account_message(self.account_id(), message_id)
            .await
    }

    /// Retrieves up to `count` messages from the mailbox.
    pub async fn mailbox_messages(
        &self,
        count: usize,
    ) -> Result<Vec<(UserStoredMessage, Interval<u32>)>> {
        self.unsigned()
            .mailbox_messages(Some(self.account_id()), count)
            .await
    }
}
