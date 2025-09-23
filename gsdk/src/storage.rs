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
    Api, BlockNumber, GearGasNode, GearGasNodeId, GearPages,
    metadata::{
        runtime_types::{
            frame_system::{AccountInfo, EventRecord},
            gear_common::storage::primitives::Interval,
            gear_core::{
                code::{instrumented::InstrumentedCode, metadata::CodeMetadata},
                message::user::UserStoredMessage,
                program::{ActiveProgram, Program},
            },
            pallet_balances::types::AccountData,
            pallet_gear_bank::pallet::BankAccount,
        },
        storage::{
            GearBankStorage, GearGasStorage, GearMessengerStorage, GearProgramStorage, GearStorage,
            SessionStorage, SystemStorage, TimestampStorage,
        },
        vara_runtime::RuntimeEvent,
    },
    result::{Error, Result},
    utils,
};
use anyhow::anyhow;
use gear_core::ids::*;
use gsdk_codegen::storage_fetch;
use hex::ToHex;
use sp_core::{H256, crypto::Ss58Codec};
use sp_runtime::AccountId32;
use std::collections::HashMap;
use subxt::{
    dynamic::{DecodedValueThunk, Value},
    ext::codec::{Decode, Encode},
    metadata::types::StorageEntryType,
    storage::{Address, DynamicAddress, StaticStorageKey},
    utils::Yes,
};

impl Api {
    /// Shortcut for fetching storage at specified block.
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
    ///     let bn = api.fetch_storage(address).await?;
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
    pub async fn fetch_storage_at<'a, Addr, Value>(
        &self,
        address: &'a Addr,
        block_hash: Option<H256>,
    ) -> Result<Value>
    where
        Addr: Address<IsFetchable = Yes, IsDefaultable = Yes, Target = DecodedValueThunk> + 'a,
        Value: Decode,
    {
        let client = self.storage();
        let storage = if let Some(h) = block_hash {
            client.at(h)
        } else {
            client.at_latest().await?
        };

        Ok(Value::decode(
            &mut storage
                .fetch(address)
                .await?
                .ok_or(Error::StorageNotFound)?
                .encoded(),
        )?)
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
        let addr = Self::storage(SystemStorage::Account, vec![Value::from_bytes(dest)]);

        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        let addr = Self::storage_root(SystemStorage::Number);
        self.fetch_storage(&addr).await
    }

    /// Get balance by account address.
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }

    /// Get events at specified block.
    #[storage_fetch]
    pub async fn get_events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
        let addr = Self::storage_root(SystemStorage::Events);

        let evs: Vec<EventRecord<RuntimeEvent, H256>> =
            self.fetch_storage_at(&addr, block_hash).await?;

        Ok(evs.into_iter().map(|ev| ev.event).collect())
    }
}

// pallet-timestamp
impl Api {
    /// Return a timestamp of the block.
    pub async fn block_timestamp(&self, block_hash: Option<H256>) -> Result<u64> {
        let addr = Self::storage_root(TimestampStorage::Now);
        self.fetch_storage_at(&addr, block_hash).await
    }
}

// pallet-session
impl Api {
    /// Get all validators from pallet_session.
    pub async fn validators(&self) -> Result<Vec<AccountId32>> {
        let addr = Self::storage_root(SessionStorage::Validators);
        self.fetch_storage(&addr).await
    }
}

// pallet-gas
impl Api {
    /// Get value of gas total issuance at specified block.
    #[storage_fetch]
    pub async fn total_issuance_at(&self, block_hash: Option<H256>) -> Result<u64> {
        let addr = Self::storage_root(GearGasStorage::TotalIssuance);
        self.fetch_storage_at(&addr, block_hash).await
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
            let addr = Self::storage(GearGasStorage::GasNodes, StaticStorageKey::new(gas_node_id));
            let gas_node = self.fetch_storage_at(&addr, block_hash).await?;
            gas_nodes.push((*gas_node_id, gas_node));
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
        let addr = Self::storage(GearBankStorage::Bank, vec![Value::from_bytes(account_id)]);

        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get Gear bank's sovereign account id.
    pub async fn bank_address(&self) -> Result<AccountId32> {
        let addr = Self::storage_root(GearBankStorage::BankAddress);
        let thunk = self
            .get_storage(None)
            .await?
            .fetch(&addr)
            .await?
            .ok_or(Error::StorageNotFound)?
            .into_encoded();
        Ok(AccountId32::decode(&mut thunk.as_ref())?)
    }
}

// pallet-gear
impl Api {
    /// Check whether the message queue processing is stopped or not.
    pub async fn execute_inherent(&self) -> Result<bool> {
        let addr = Self::storage_root(GearStorage::ExecuteInherent);
        let thunk = self
            .get_storage(None)
            .await?
            .fetch_or_default(&addr)
            .await?
            .into_encoded();

        Ok(bool::decode(&mut thunk.as_ref())?)
    }

    /// Get gear block number.
    pub async fn gear_block_number(&self, block_hash: Option<H256>) -> Result<BlockNumber> {
        let addr = Self::storage_root(GearStorage::BlockNumber);
        let thunk = self
            .get_storage(block_hash)
            .await?
            .fetch_or_default(&addr)
            .await?
            .into_encoded();
        Ok(BlockNumber::decode(&mut thunk.as_ref())?)
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
        let addr = Self::storage(
            GearProgramStorage::OriginalCodeStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get `InstrumentedCode` by its `CodeId` at specified block.
    #[storage_fetch]
    pub async fn instrumented_code_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<InstrumentedCode> {
        let addr = Self::storage(
            GearProgramStorage::InstrumentedCodeStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get `CodeMetadata` by its `CodeId` at specified block.
    #[storage_fetch]
    pub async fn code_metadata_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<CodeMetadata> {
        let addr = Self::storage(
            GearProgramStorage::CodeMetadataStorage,
            vec![Value::from_bytes(code_id)],
        );
        self.fetch_storage_at(&addr, block_hash).await
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
        memory_infix: Option<u32>,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        let mut pages = HashMap::new();

        let memory_infix = match memory_infix {
            Some(infix) => infix,
            None => self.gprog_at(program_id, block_hash).await?.memory_infix.0,
        };

        let pages_storage_address = Self::storage(
            GearProgramStorage::MemoryPages,
            vec![
                Value::from_bytes(program_id),
                Value::u128(memory_infix as u128),
            ],
        );

        let mut pages_stream = self
            .get_storage(block_hash)
            .await?
            .iter(pages_storage_address)
            .await?;

        while let Some(Ok(pair)) = pages_stream.next().await {
            let (_, page) = <([u8; 68], u32)>::decode(&mut pair.key_bytes.as_ref())?;
            let data = pair.value.encoded().to_vec();
            pages.insert(page, data);
        }

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
        memory_infix: Option<u32>,
        page_numbers: impl Iterator<Item = u32>,
        block_hash: Option<H256>,
    ) -> Result<GearPages> {
        let mut pages = HashMap::new();

        let memory_infix = match memory_infix {
            Some(infix) => infix,
            None => self.gprog_at(program_id, block_hash).await?.memory_infix.0,
        };

        for page in page_numbers {
            let addr: DynamicAddress<Vec<Value>> = Self::storage(
                GearProgramStorage::MemoryPages,
                vec![
                    Value::from_bytes(program_id),
                    Value::u128(memory_infix as u128),
                    Value::u128(page as u128),
                ],
            );

            let metadata = self.metadata();
            let lookup_bytes = utils::storage_address_bytes(&addr, &metadata)?;
            let encoded_page = self
                .get_storage(block_hash)
                .await?
                .fetch_raw(lookup_bytes)
                .await?
                .ok_or_else(|| Error::PageNotFound(page, program_id.as_ref().encode_hex()))?;

            pages.insert(page, encoded_page);
        }

        Ok(pages)
    }

    /// Get program by its id at specified block.
    #[storage_fetch]
    pub async fn program_at(
        &self,
        program_id: ActorId,
        block_hash: Option<H256>,
    ) -> Result<Program<BlockNumber>> {
        let addr = Self::storage(
            GearProgramStorage::ProgramStorage,
            vec![Value::from_bytes(program_id)],
        );

        self.fetch_storage_at::<_, Program<BlockNumber>>(&addr, block_hash)
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
        message_id: impl AsRef<[u8]>,
    ) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        let addr = Self::storage(
            GearMessengerStorage::Mailbox,
            vec![
                Value::from_bytes(account_id),
                Value::from_bytes(message_id.as_ref()),
            ],
        );

        Ok(self.fetch_storage(&addr).await.ok())
    }

    /// Get all mailbox messages or for the provided `address`.
    pub async fn mailbox(
        &self,
        account_id: Option<AccountId32>,
        count: u32,
    ) -> Result<Vec<(UserStoredMessage, Interval<u32>)>> {
        let storage = self.storage().at_latest().await?;
        let mut query_key = Self::storage_root(GearMessengerStorage::Mailbox).to_root_bytes();

        if let Some(account_id) = account_id {
            query_key.extend(account_id.encode());
        }

        let mut keys = storage.fetch_raw_keys(query_key).await?;
        let mut mailbox: Vec<(UserStoredMessage, Interval<u32>)> = vec![];

        let mut fetched_keys = 0;
        while let Some(key) = keys.next().await {
            if let Some(storage_data) = storage.fetch_raw(key?).await?
                && let Ok(value) =
                    <(UserStoredMessage, Interval<u32>)>::decode(&mut &storage_data[..])
            {
                mailbox.push(value);
            }

            fetched_keys += 1;
            if fetched_keys >= count {
                break;
            }
        }

        Ok(mailbox)
    }
}

/// Get storage entry type id using `metadata` and storage entry `address`
pub(crate) fn storage_type_id(metadata: &subxt::Metadata, address: &impl Address) -> Result<u32> {
    let storage_type = metadata
        .pallet_by_name_err(address.pallet_name())?
        .storage()
        .ok_or(anyhow!("Storage {} not found", address.pallet_name()))?
        .entry_by_name(address.entry_name())
        .ok_or(anyhow!("Entry {} not found", address.entry_name()))?
        .entry_type();

    let storage_type_id = match storage_type {
        StorageEntryType::Plain(id) => id,
        StorageEntryType::Map { value_ty, .. } => value_ty,
    };

    Ok(*storage_type_id)
}
