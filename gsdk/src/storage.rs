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

//! Gear storage apis
use futures::prelude::*;

use crate::{
    Api, BlockNumber, GearGasNode, GearGasNodeId, GearPages, IntoAccountId32, IntoSubstrate,
    gear::{
        self,
        runtime_types::{
            frame_system::{AccountInfo, EventRecord},
            gear_common::storage::primitives::Interval,
            gear_core::{
                pages::Page,
                program::{ActiveProgram, Program},
            },
            pallet_balances::types::AccountData,
            pallet_gear_bank::pallet::BankAccount,
            vara_runtime::RuntimeEvent,
        },
    },
    result::{Error, FailedPage, Result},
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId, MessageId},
    message::UserStoredMessage,
    pages::GearPage,
    program::MemoryInfix,
};
use gsdk_codegen::at_block;
use sp_core::crypto::AccountId32;
use subxt::{
    error::MetadataError,
    ext::subxt_core::storage::address::StorageHashers,
    metadata::types::StorageEntryType,
    storage::{Address, StaticStorageKey, StorageKey},
    utils::{H256, Yes},
};

impl Api {
    /// Shortcut for fetching a value from storage at specified block.
    ///
    /// # You may not need this.
    ///
    /// Read the docs of [`Api`] to checkout the wrapped storage queries,
    /// we need this function only when we want to execute a query which
    /// has not been wrapped in `gsdk`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use gsdk::{Api, metadata::storage::SystemStorage};
    ///
    /// let api = Api::new(None);
    ///
    /// {
    ///     let address = Api::storage(SystemStorage::Number);
    ///     let bn = api.storage_fetch(address).await?;
    /// }
    ///
    /// // The code above equals to the following code due to
    /// // the implemented storage query `number` in `Api`.
    ///
    /// {
    ///     let bn = api.number().await?;
    /// }
    /// ```
    #[at_block]
    pub async fn storage_fetch_at<'a, Addr>(
        &self,
        address: &'a Addr,
        block_hash: Option<H256>,
    ) -> Result<Addr::Target>
    where
        Addr: Address<IsFetchable = Yes> + 'a,
    {
        self.storage_at(block_hash)
            .await?
            .fetch(address)
            .await?
            .ok_or(Error::StorageEntryNotFound)
    }
}

// frame-system
impl Api {
    /// Get account info by its address at specified block.
    #[at_block]
    pub async fn account_info_at(
        &self,
        address: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<AccountInfo<u32, AccountData<u128>>> {
        self.storage_fetch_at(
            &gear::storage().system().account(address.into_account_id()),
            block_hash,
        )
        .await
    }

    /// Get account data by its address at specified block.
    #[at_block]
    pub async fn account_data_at(
        &self,
        address: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<AccountData<u128>> {
        self.account_info_at(address, block_hash)
            .map_ok(|info| info.data)
            .await
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        self.storage_fetch(&gear::storage().system().number()).await
    }

    /// Get the free funds balance of the account identified by `account_id` at specified block.
    #[at_block]
    pub async fn free_balance_at(
        &self,
        account_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<u128> {
        Ok(self.account_data_at(account_id, block_hash).await?.free)
    }

    /// Get the reserved funds balance of the account identified by `account_id` at specified block.
    #[at_block]
    pub async fn reserved_balance_at(
        &self,
        account_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<u128> {
        Ok(self.account_data_at(account_id, block_hash).await?.reserved)
    }

    /// Get the total balance of the account identified by `account_id` at specified block.
    ///
    /// Total balance includes free and reserved funds.
    #[at_block]
    pub async fn total_balance_at(
        &self,
        account_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<u128> {
        let data = self.account_data_at(account_id, block_hash).await?;

        data.free
            .checked_add(data.reserved)
            .ok_or(Error::BalanceOverflow)
    }

    /// Get events at specified block.
    #[at_block]
    pub async fn events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
        let addr = gear::storage().system().events();

        let evs: Vec<EventRecord<RuntimeEvent, H256>> =
            self.storage_fetch_at(&addr, block_hash).await?;

        Ok(evs.into_iter().map(|ev| ev.event).collect())
    }
}

// pallet-timestamp
impl Api {
    /// Return a timestamp of the block.
    pub async fn block_timestamp(&self, block_hash: Option<H256>) -> Result<u64> {
        self.storage_fetch_at(&gear::storage().timestamp().now(), block_hash)
            .await
    }
}

// pallet-session
impl Api {
    /// Get all validators from pallet_session.
    pub async fn validators(&self) -> Result<Vec<AccountId32>> {
        Ok(self
            .storage_fetch(&gear::storage().session().validators())
            .await?
            .into_iter()
            .map(|id| id.into_substrate())
            .collect())
    }
}

// pallet-gas
impl Api {
    /// Get value of gas total issuance at specified block.
    #[at_block]
    pub async fn total_issuance_at(&self, block_hash: Option<H256>) -> Result<u64> {
        self.storage_fetch_at(&gear::storage().gear_gas().total_issuance(), block_hash)
            .await
    }

    /// Get Gear gas nodes by their ids at specified block.
    #[at_block]
    pub async fn gas_nodes_at(
        &self,
        gas_node_ids: impl IntoIterator<Item = GearGasNodeId>,
        block_hash: Option<H256>,
    ) -> Result<Vec<(GearGasNodeId, GearGasNode)>> {
        stream::iter(gas_node_ids)
            .then(|gas_node_id| async move {
                let addr = gear::storage().gear_gas().gas_nodes(gas_node_id.clone());
                let gas_node = self.storage_fetch_at(&addr, block_hash).await?;

                Ok((gas_node_id.clone(), gas_node))
            })
            .try_collect()
            .await
    }
}

// pallet-gear-bank
impl Api {
    /// Get Gear bank account data at specified block.
    #[at_block]
    pub async fn bank_info_at(
        &self,
        account_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<BankAccount<u128>> {
        self.storage_fetch_at(
            &gear::storage()
                .gear_bank()
                .bank(account_id.into_account_id()),
            block_hash,
        )
        .await
    }

    /// Get Gear bank's sovereign account id.
    pub async fn bank_address(&self) -> Result<AccountId32> {
        Ok(self
            .storage_fetch(&gear::storage().gear_bank().bank_address())
            .await?
            .into_substrate())
    }
}

// pallet-gear
impl Api {
    /// Check whether the message queue processing is stopped or not.
    pub async fn execute_inherent(&self) -> Result<bool> {
        Ok(self
            .storage()
            .at_latest()
            .await?
            .fetch_or_default(&gear::storage().gear().execute_inherent())
            .await?)
    }

    /// Get gear block number.
    pub async fn gear_block_number(&self, block_hash: Option<H256>) -> Result<BlockNumber> {
        Ok(self
            .storage_at(block_hash)
            .await?
            .fetch_or_default(&gear::storage().gear().block_number())
            .await?)
    }
}

// pallet-gear-program
impl Api {
    /// Returns original WASM code for the given `CodeId` at specified block.
    #[at_block]
    pub async fn original_code_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<Vec<u8>> {
        self.storage_fetch_at(
            &gear::storage()
                .gear_program()
                .original_code_storage(code_id),
            block_hash,
        )
        .await
    }

    /// Get `InstrumentedCode` by its `CodeId` at specified block.
    #[at_block]
    pub async fn instrumented_code_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<InstrumentedCode> {
        self.storage_fetch_at(
            &gear::storage()
                .gear_program()
                .instrumented_code_storage(code_id),
            block_hash,
        )
        .await
    }

    /// Get `CodeMetadata` by its `CodeId` at specified block.
    #[at_block]
    pub async fn code_metadata_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<CodeMetadata> {
        self.storage_fetch_at(
            &gear::storage()
                .gear_program()
                .code_metadata_storage(code_id),
            block_hash,
        )
        .await
    }

    /// Returns `ActiveProgram` for the given `ActorId` at specified block.
    #[at_block]
    pub async fn active_program_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<ActiveProgram<BlockNumber>> {
        match self.program_at(program_id, block_hash).await? {
            Program::Active(p) => Ok(p),
            _ => Err(Error::ProgramTerminated),
        }
    }

    /// Get pages of an active program at specified block.
    #[at_block]
    pub async fn program_pages_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        let address = gear::storage()
            .gear_program()
            .memory_pages_iter1(program_id);

        let metadata = self.metadata();
        let hashers = subxt::ext::subxt_core::storage::lookup_storage_entry_details(
            address.pallet_name(),
            address.entry_name(),
            &metadata,
        )
        .and_then(|(_, entry)| StorageHashers::new(entry.entry_type(), metadata.types()))
        .map_err(subxt::Error::from)?;
        let pages = self
            .storage_at(block_hash)
            .await?
            .iter(address)
            .try_flatten_stream()
            .map_err(Error::from)
            .and_then(|pair| {
                std::future::ready({
                    // FIXME: Do not decode key manually.
                    //        Requires a fix from `subxt`.
                    <(
                        StaticStorageKey<ActorId>,
                        StaticStorageKey<MemoryInfix>,
                        StaticStorageKey<Page>,
                    ) as StorageKey>::decode_storage_key(
                        &mut &pair.key_bytes[32..],
                        &mut hashers.iter(),
                        metadata.types(),
                    )
                    .map_err(subxt::Error::from)
                    .map_err(Error::from)
                    .and_then(|(_, _, page_index)| {
                        Ok((page_index.into_key().0.try_into()?, pair.value))
                    })
                })
            })
            .try_collect()
            .await?;

        Ok(pages)
    }

    /// Get inheritor address by program id at specified block.
    #[at_block]
    pub async fn inheritor_of_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<Option<ActorId>> {
        Ok(match self.program_at(program_id, block_hash).await? {
            Program::Exited(p) => Some(p),
            _ => None,
        })
    }

    /// Get pages of active program at specified block.
    #[at_block]
    pub async fn specified_program_pages_at(
        &self,
        program_id: ActorId,
        page_numbers: impl IntoIterator<Item = GearPage>,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        futures::stream::iter(page_numbers)
            .then(|page| async move {
                let addr = gear::storage().gear_program().memory_pages(
                    program_id,
                    // memory infix is always zero now
                    MemoryInfix::default(),
                    page.into(),
                );

                let page_buf = self
                    .storage_at(block_hash)
                    .await?
                    .fetch(&addr)
                    .await?
                    .ok_or_else(|| FailedPage::new(page, program_id).not_found())?;

                Ok((page, page_buf))
            })
            .try_collect()
            .await
    }

    /// Get program by its id at specified block.
    #[at_block]
    pub async fn program_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<Program<BlockNumber>> {
        self.storage_fetch_at(
            &gear::storage().gear_program().program_storage(program_id),
            block_hash,
        )
        .await
    }
}

// pallet-gear-messenger
impl Api {
    /// Get a message identified by `message_id` from the `account_id`'s
    /// mailbox.
    pub async fn mailbox_account_message(
        &self,
        account_id: impl IntoAccountId32,
        message_id: MessageId,
    ) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        Ok(self
            .storage_fetch(
                &gear::storage()
                    .gear_messenger()
                    .mailbox(account_id.into_account_id(), message_id),
            )
            .await
            .ok())
    }

    /// Get all mailbox messages or for the provided `address`.
    pub async fn mailbox_messages(
        &self,
        account_id: Option<impl IntoAccountId32>,
        count: usize,
    ) -> Result<Vec<(UserStoredMessage, Interval<u32>)>> {
        let storage = self.storage().at_latest().await?;

        if let Some(account_id) = account_id {
            let query_key = gear::storage()
                .gear_messenger()
                .mailbox_iter1(account_id.into_account_id());
            storage
                .iter(query_key)
                .await?
                .map_ok(|pair| pair.value)
                .boxed()
        } else {
            let query_key = gear::storage().gear_messenger().mailbox_iter();
            storage
                .iter(query_key)
                .await?
                .map_ok(|pair| pair.value)
                .boxed()
        }
        .take(count)
        .try_collect()
        .await
        .map_err(Error::from)
    }
}

/// Get storage entry type id using `metadata` and storage entry `address`
pub(crate) fn storage_type_id(
    metadata: &subxt::Metadata,
    address: &impl Address,
) -> Result<u32, MetadataError> {
    let storage_type = metadata
        .pallet_by_name_err(address.pallet_name())?
        .storage()
        .ok_or_else(|| MetadataError::StorageNotFoundInPallet(address.pallet_name().to_owned()))?
        .entry_by_name(address.entry_name())
        .ok_or_else(|| MetadataError::StorageEntryNotFound(address.entry_name().to_owned()))?
        .entry_type();

    let storage_type_id = *match storage_type {
        StorageEntryType::Plain(id) => id,
        StorageEntryType::Map { value_ty, .. } => value_ty,
    };

    Ok(storage_type_id)
}
