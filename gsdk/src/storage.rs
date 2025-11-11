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
use crate::{
    Api, BlockNumber, GearGasNode, GearGasNodeId, GearPages, IntoSubstrate, IntoSubxt,
    gear::{
        self,
        runtime_types::{
            frame_system::{AccountInfo, EventRecord},
            gear_common::storage::primitives::Interval,
            gear_core::{
                code::{instrumented::InstrumentedCode, metadata::CodeMetadata},
                message::user::UserStoredMessage,
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
use futures::prelude::*;
use gear_core::{ids::*, program::MemoryInfix};
use gsdk_codegen::storage_fetch;
use hex::ToHex;
use sp_core::crypto::{AccountId32, Ss58Codec};
use std::collections::HashMap;
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
    #[storage_fetch]
    pub async fn storage_fetch_at<'a, Addr>(
        &self,
        address: &'a Addr,
        block_hash: Option<H256>,
    ) -> Result<Addr::Target>
    where
        Addr: Address<IsFetchable = Yes> + 'a,
    {
        let client = self.storage();
        let storage = if let Some(h) = block_hash {
            client.at(h)
        } else {
            client.at_latest().await?
        };

        Ok(storage
            .fetch(address)
            .await?
            .ok_or(Error::StorageEntryNotFound)?)
    }

    /// Get program pages from program id.
    pub async fn program_pages(&self, program_id: ActorId) -> Result<GearPages> {
        self.gpages(program_id, None).await
    }
}

// frame-system
impl Api {
    /// Get account info by address at specified block.
    #[storage_fetch]
    pub async fn info_at(
        &self,
        address: &str,
        block_hash: Option<H256>,
    ) -> Result<AccountInfo<u32, AccountData<u128>>> {
        let dest = AccountId32::from_ss58check(address)?;
        let addr = gear::storage().system().account(dest.into_subxt());

        self.storage_fetch_at(&addr, block_hash).await
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        self.storage_fetch(&gear::storage().system().number()).await
    }

    /// Get balance by account address.
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }

    /// Get events at specified block.
    #[storage_fetch]
    pub async fn get_events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
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
    #[storage_fetch]
    pub async fn total_issuance_at(&self, block_hash: Option<H256>) -> Result<u64> {
        self.storage_fetch_at(&gear::storage().gear_gas().total_issuance(), block_hash)
            .await
    }

    /// Get Gear gas nodes by their ids at specified block.
    #[storage_fetch]
    pub async fn gas_nodes_at(
        &self,
        gas_node_ids: &impl AsRef<[GearGasNodeId]>,
        block_hash: Option<H256>,
    ) -> Result<Vec<(GearGasNodeId, GearGasNode)>> {
        let gas_node_ids = gas_node_ids.as_ref();
        let mut gas_nodes = Vec::with_capacity(gas_node_ids.len());

        for gas_node_id in gas_node_ids {
            let addr = gear::storage().gear_gas().gas_nodes(gas_node_id.clone());
            let gas_node = self.storage_fetch_at(&addr, block_hash).await?;
            gas_nodes.push((gas_node_id.clone(), gas_node));
        }
        Ok(gas_nodes)
    }
}

// pallet-gear-bank
impl Api {
    /// Get Gear bank account data at specified block.
    #[storage_fetch]
    pub async fn bank_info_at(
        &self,
        account_id: AccountId32,
        block_hash: Option<H256>,
    ) -> Result<BankAccount<u128>> {
        self.storage_fetch_at(
            &gear::storage().gear_bank().bank(account_id.into_subxt()),
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
        let addr = gear::storage().gear().execute_inherent();
        Ok(self
            .storage()
            .at_latest()
            .await?
            .fetch_or_default(&addr)
            .await?)
    }

    /// Get gear block number.
    pub async fn gear_block_number(&self, block_hash: Option<H256>) -> Result<BlockNumber> {
        let addr = gear::storage().gear().block_number();
        Ok(self
            .storage_at(block_hash)
            .await?
            .fetch_or_default(&addr)
            .await?)
    }
}

// pallet-gear-program
impl Api {
    /// Get original code by its `CodeId` at specified block.
    #[storage_fetch]
    pub async fn original_code_storage_at(
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
    #[storage_fetch]
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
    #[storage_fetch]
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

    /// Get active program from program id at specified block.
    #[storage_fetch]
    pub async fn gprog_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<ActiveProgram<BlockNumber>> {
        match self.program_at(program_id, block_hash).await? {
            Program::Active(p) => Ok(p),
            _ => Err(Error::ProgramTerminated),
        }
    }

    /// Get pages of active program at specified block.
    #[storage_fetch]
    pub async fn gpages_at(
        &self,
        program_id: ActorId,
        memory_infix: Option<MemoryInfix>,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        let memory_infix = match memory_infix {
            Some(infix) => infix,
            None => self.gprog_at(program_id, block_hash).await?.memory_infix,
        };

        let address = gear::storage()
            .gear_program()
            .memory_pages_iter2(program_id, memory_infix);

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
                    .map(|(_, _, page_index)| (page_index.into_key().0, pair.value))
                    .map_err(subxt::Error::from)
                })
            })
            .try_collect()
            .await?;

        Ok(pages)
    }

    /// Get inheritor address by program id at specified block.
    #[storage_fetch]
    pub async fn inheritor_of_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<Option<ActorId>> {
        Ok(match self.program_at(program_id, block_hash).await? {
            Program::Exited(p) => Some(p.into()),
            _ => None,
        })
    }

    /// Get pages of active program at specified block.
    #[storage_fetch]
    pub async fn specified_gpages_at(
        &self,
        program_id: ActorId,
        memory_infix: Option<MemoryInfix>,
        page_numbers: impl IntoIterator<Item = u32>,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        let memory_infix = match memory_infix {
            Some(infix) => infix,
            None => self.gprog_at(program_id, block_hash).await?.memory_infix,
        };

        futures::stream::iter(page_numbers)
            .then(|page| async move {
                let addr = gear::storage().gear_program().memory_pages(
                    program_id,
                    memory_infix,
                    Page(page),
                );

                let page_buf = self
                    .storage_at(block_hash)
                    .await?
                    .fetch(&addr)
                    .await?
                    .ok_or_else(|| {
                        FailedPage::new(page, program_id.as_ref().encode_hex()).not_found()
                    })?;

                Ok((page, page_buf))
            })
            .try_collect()
            .await
    }

    /// Get program by its id at specified block.
    #[storage_fetch]
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
    pub async fn get_mailbox_account_message(
        &self,
        account_id: AccountId32,
        message_id: MessageId,
    ) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        Ok(self
            .storage_fetch(
                &gear::storage()
                    .gear_messenger()
                    .mailbox(account_id.into_subxt(), message_id),
            )
            .await
            .ok())
    }

    /// Get all mailbox messages or for the provided `address`.
    pub async fn mailbox(
        &self,
        account_id: Option<AccountId32>,
        count: usize,
    ) -> Result<Vec<(UserStoredMessage, Interval<u32>)>> {
        let storage = self.storage().at_latest().await?;

        if let Some(account_id) = account_id {
            let query_key = gear::storage()
                .gear_messenger()
                .mailbox_iter1(account_id.into_subxt());
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
