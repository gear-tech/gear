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

//! Gear storage apis
use crate::{
    metadata::runtime_types::{
        frame_system::{AccountInfo, EventRecord},
        gear_common::{storage::primitives::Interval, ActiveProgram, Program},
        gear_core::{code::InstrumentedCode, message::stored::StoredMessage},
        gear_runtime::RuntimeEvent,
        pallet_balances::AccountData,
    },
    result::{Error, Result},
    types, Api,
};
use gear_core::ids::*;
use hex::ToHex;
use parity_scale_codec::Decode;
use sp_core::{crypto::Ss58Codec, H256};
use sp_runtime::AccountId32;
use std::collections::HashMap;
use subxt::{
    dynamic::{DecodedValueThunk, Value},
    storage::{
        address::{StorageAddress, StorageHasher, StorageMapKey, Yes},
        utils::storage_address_root_bytes,
    },
};

impl Api {
    /// Shortcut for fetching storage.
    pub async fn fetch_storage<'a, Address, Value>(&self, address: &'a Address) -> Result<Value>
    where
        Address:
            StorageAddress<IsFetchable = Yes, IsDefaultable = Yes, Target = DecodedValueThunk> + 'a,
        Value: Decode,
    {
        self.fetch_storage_at(address, None).await
    }

    /// Shortcut for fetching storage at block specified by its hash.
    pub async fn fetch_storage_at<'a, Address, Value>(
        &self,
        address: &'a Address,
        block_hash: Option<H256>,
    ) -> Result<Value>
    where
        Address:
            StorageAddress<IsFetchable = Yes, IsDefaultable = Yes, Target = DecodedValueThunk> + 'a,
        Value: Decode,
    {
        Ok(Value::decode(
            &mut self
                .storage()
                .at(block_hash)
                .await?
                .fetch(address)
                .await?
                .ok_or(Error::StorageNotFound)?
                .encoded(),
        )?)
    }

    /// Get program pages from program id.
    pub async fn program_pages(&self, pid: ProgramId) -> Result<types::GearPages> {
        self.gpages(pid, &self.gprog(pid).await?).await
    }
}

// frame-system
impl Api {
    /// Get account info by address.
    pub async fn info(&self, address: &str) -> Result<AccountInfo<u32, AccountData<u128>>> {
        self.info_at(address, None).await
    }

    /// Get account info by address at specified block.
    pub async fn info_at(
        &self,
        address: &str,
        block_hash: Option<H256>,
    ) -> Result<AccountInfo<u32, AccountData<u128>>> {
        let dest = AccountId32::from_ss58check(address)?;
        let addr = subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(dest)]);

        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        let addr = subxt::dynamic::storage_root("System", "Number");
        self.fetch_storage(&addr).await
    }

    /// Get balance by account address.
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }

    /// Get events from the block.
    pub async fn get_events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
        let addr = subxt::dynamic::storage_root("System", "Events");
        let thunk = self
            .storage()
            .at(block_hash)
            .await?
            .fetch(&addr)
            .await?
            .ok_or(Error::StorageNotFound)?
            .into_encoded();

        Ok(
            Vec::<EventRecord<RuntimeEvent, H256>>::decode(&mut thunk.as_ref())?
                .into_iter()
                .map(|ev| ev.event)
                .collect(),
        )
    }
}

// pallet-timestamp
impl Api {
    /// Return a timestamp of the block.
    pub async fn block_timestamp(&self, block_hash: Option<H256>) -> Result<u64> {
        let addr = subxt::dynamic::storage_root("Timestamp", "now");
        let thunk = self
            .storage()
            .at(block_hash)
            .await?
            .fetch(&addr)
            .await?
            .ok_or(Error::StorageNotFound)?
            .into_encoded();

        Ok(u64::decode(&mut thunk.as_ref())?)
    }
}

// pallet-session
impl Api {
    /// Get all validators from pallet_session.
    pub async fn validators(&self) -> Result<Vec<AccountId32>> {
        let addr = subxt::dynamic::storage_root("Session", "Validators");
        self.fetch_storage(&addr).await
    }
}

// pallet-gas
impl Api {
    /// Get value of gas total issuance.
    pub async fn total_issuance(&self) -> Result<u64> {
        self.total_issuance_at(None).await
    }

    /// Get value of gas total issuance at specified block.
    pub async fn total_issuance_at(&self, block_hash: Option<H256>) -> Result<u64> {
        let addr = subxt::dynamic::storage_root("GearGas", "TotalIssuance");
        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get Gear gas nodes by their ids.
    pub async fn gas_nodes(
        &self,
        gas_node_ids: &impl AsRef<[types::GearGasNodeId]>,
    ) -> Result<Vec<(types::GearGasNodeId, types::GearGasNode)>> {
        self.gas_nodes_at(gas_node_ids, None).await
    }

    /// Get Gear gas nodes by their ids at specified block.
    pub async fn gas_nodes_at(
        &self,
        gas_node_ids: &impl AsRef<[types::GearGasNodeId]>,
        block_hash: Option<H256>,
    ) -> Result<Vec<(types::GearGasNodeId, types::GearGasNode)>> {
        let gas_node_ids = gas_node_ids.as_ref();
        let mut gas_nodes = Vec::with_capacity(gas_node_ids.len());
        for gas_node_id in gas_node_ids {
            let addr = subxt::dynamic::storage(
                "GearGas",
                "GasNodes",
                vec![subxt::metadata::EncodeStaticType(gas_node_id)],
            );
            let gas_node = self.fetch_storage_at(&addr, block_hash).await?;
            gas_nodes.push((*gas_node_id, gas_node));
        }
        Ok(gas_nodes)
    }
}

// pallet-gear
impl Api {
    /// Check whether the message queue processing is stopped or not.
    pub async fn execute_inherent(&self) -> Result<bool> {
        let addr = subxt::dynamic::storage_root("Gear", "ExecuteInherent");
        let thunk = self
            .storage()
            .at(None)
            .await?
            .fetch_or_default(&addr)
            .await?
            .into_encoded();

        Ok(bool::decode(&mut thunk.as_ref())?)
    }

    /// Get gear block number.
    pub async fn gear_block_number(&self, block_hash: Option<H256>) -> Result<u32> {
        let addr = subxt::dynamic::storage_root("Gear", "BlockNumber");
        let thunk = self
            .storage()
            .at(block_hash)
            .await?
            .fetch_or_default(&addr)
            .await?
            .into_encoded();

        Ok(u32::decode(&mut thunk.as_ref())?)
    }
}

// pallet-gear-program
impl Api {
    /// Get `InstrumentedCode` by its `CodeId`
    pub async fn code_storage(&self, code_id: CodeId) -> Result<InstrumentedCode> {
        self.code_storage_at(code_id, None).await
    }

    /// Get `InstrumentedCode` by its `CodeId` at specified block.
    pub async fn code_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<InstrumentedCode> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeStorage",
            vec![Value::from_bytes(code_id)],
        );
        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get `InstrumentedCode` length by its `CodeId`
    pub async fn code_len_storage(&self, code_id: CodeId) -> Result<u32> {
        self.code_len_storage_at(code_id, None).await
    }

    /// Get `InstrumentedCode` length by its `CodeId` at specified block.
    pub async fn code_len_storage_at(
        &self,
        code_id: CodeId,
        block_hash: Option<H256>,
    ) -> Result<u32> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeLenStorage",
            vec![Value::from_bytes(code_id)],
        );
        self.fetch_storage_at(&addr, block_hash).await
    }

    /// Get active program from program id.
    pub async fn gprog(&self, program_id: ProgramId) -> Result<ActiveProgram> {
        self.gprog_at(program_id, None).await
    }

    /// Get active program from program id at specified block.
    pub async fn gprog_at(
        &self,
        program_id: ProgramId,
        block_hash: Option<H256>,
    ) -> Result<ActiveProgram> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "ProgramStorage",
            vec![Value::from_bytes(program_id)],
        );

        let program = self
            .fetch_storage_at::<_, (Program, u32)>(&addr, block_hash)
            .await?
            .0;

        match program {
            Program::Active(p) => Ok(p),
            _ => Err(Error::ProgramTerminated),
        }
    }

    /// Get pages of active program.
    pub async fn gpages(
        &self,
        program_id: ProgramId,
        program: &ActiveProgram,
    ) -> Result<types::GearPages> {
        self.gpages_at(program_id, program, None).await
    }

    /// Get pages of active program at specified block.
    pub async fn gpages_at(
        &self,
        program_id: ProgramId,
        program: &ActiveProgram,
        block_hash: Option<H256>,
    ) -> Result<types::GearPages> {
        let mut pages = HashMap::new();
        for page in &program.pages_with_data {
            let addr = subxt::dynamic::storage(
                "GearProgram",
                "MemoryPageStorage",
                vec![Value::from_bytes(program_id), Value::u128(page.0 as u128)],
            );

            let metadata = self.metadata();
            let lookup_bytes = subxt::storage::utils::storage_address_bytes(&addr, &metadata)?;

            let encoded_page = self
                .storage()
                .at(block_hash)
                .await?
                .fetch_raw(&lookup_bytes)
                .await?
                .ok_or_else(|| Error::PageNotFound(page.0, program_id.as_ref().encode_hex()))?;
            pages.insert(page.0, encoded_page);
        }

        Ok(pages)
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
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        let addr = subxt::dynamic::storage(
            "GearMessenger",
            "Mailbox",
            vec![
                Value::from_bytes(account_id),
                Value::from_bytes(message_id.as_ref()),
            ],
        );

        let data: Option<(StoredMessage, Interval<u32>)> = self.fetch_storage(&addr).await.ok();
        Ok(data.map(|(m, i)| (m, i)))
    }

    /// Get all mailbox messages or from the provided `address`.
    pub async fn mailbox(
        &self,
        account_id: Option<AccountId32>,
        count: u32,
    ) -> Result<Vec<(StoredMessage, Interval<u32>)>> {
        let storage = self.storage().at(None).await?;
        let mut query_key =
            storage_address_root_bytes(&subxt::dynamic::storage_root("GearMessenger", "Mailbox"));

        if let Some(account_id) = account_id {
            StorageMapKey::new(&account_id, StorageHasher::Identity).to_bytes(&mut query_key);
        }

        let keys = storage.fetch_keys(&query_key, count, None).await?;

        let mut mailbox: Vec<(StoredMessage, Interval<u32>)> = vec![];
        for key in keys {
            if let Some(storage_data) = storage.fetch_raw(&key.0).await? {
                if let Ok(value) = <(StoredMessage, Interval<u32>)>::decode(&mut &storage_data[..])
                {
                    mailbox.push(value);
                }
            }
        }

        Ok(mailbox)
    }
}

/// Get storage entry type id using `metadata` and storage entry `address`
pub(crate) fn storage_type_id(
    metadata: &subxt::Metadata,
    address: &impl StorageAddress,
) -> Result<u32> {
    // This code is taken from subxt implementation of fetching decoded storage value.
    let storage_type = &metadata
        .pallet(address.pallet_name())?
        .storage(address.entry_name())?
        .ty;
    let storage_type_id = match storage_type {
        subxt::ext::frame_metadata::StorageEntryType::Plain(ty) => ty.id,
        subxt::ext::frame_metadata::StorageEntryType::Map { value, .. } => value.id,
    };
    Ok(storage_type_id)
}
