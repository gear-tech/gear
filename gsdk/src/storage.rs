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
use gear_core::memory::GEAR_PAGE_SIZE;
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
        Ok(Value::decode(
            &mut self
                .storage()
                .at(None)
                .await?
                .fetch(address)
                .await?
                .ok_or(Error::StorageNotFound)?
                .into_encoded()
                .as_ref(),
        )?)
    }

    /// Get program pages from program id.
    pub async fn program_pages(&self, pid: H256) -> Result<types::GearPages> {
        self.gpages(pid, self.gprog(pid).await?).await
    }
}

// frame-system
impl Api {
    /// Get account info by address
    pub async fn info(&self, address: &str) -> Result<AccountInfo<u32, AccountData<u128>>> {
        let dest = AccountId32::from_ss58check(address)?;
        let addr = subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(dest)]);

        self.fetch_storage(&addr).await
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        let addr = subxt::dynamic::storage_root("System", "Number");
        self.fetch_storage(&addr).await
    }

    /// Get balance by account address
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }

    // Get events from the block
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
    /// Get `InstrumentedCode` by `code_hash`
    pub async fn code_storage(&self, code_hash: [u8; 32]) -> Result<InstrumentedCode> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeStorage",
            vec![Value::from_bytes(code_hash)],
        );
        self.fetch_storage(&addr).await
    }

    /// Get active program from program id.
    pub async fn gprog(&self, pid: H256) -> Result<ActiveProgram> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "ProgramStorage",
            vec![Value::from_bytes(pid)],
        );

        let program = self.fetch_storage::<_, (Program, u32)>(&addr).await?.0;

        match program {
            Program::Active(p) => Ok(p),
            _ => Err(Error::ProgramTerminated),
        }
    }

    /// Get pages of active program.
    pub async fn gpages(&self, pid: H256, program: ActiveProgram) -> Result<types::GearPages> {
        let mut pages = HashMap::new();
        for page in program.pages_with_data {
            let addr = subxt::dynamic::storage(
                "GearProgram",
                "MemoryPageStorage",
                vec![Value::from_bytes(pid), Value::u128(page.0 as u128)],
            );

            let metadata = self.metadata();
            let lookup_bytes = subxt::storage::utils::storage_address_bytes(&addr, &metadata)?;

            let encoded_page = self
                .storage()
                .at(None)
                .await?
                .fetch_raw(&lookup_bytes)
                .await?
                .ok_or_else(|| Error::PageNotFound(page.0, pid.encode_hex()))?;
            let decoded = <[u8; GEAR_PAGE_SIZE]>::decode(&mut &encoded_page[..])?;
            pages.insert(page.0, decoded.to_vec());
        }

        Ok(pages)
    }
}

// pallet-gear-messenger
impl Api {
    /// Get a message identified by `message_id` from the `account_id`'s
    /// mailbox.
    pub async fn get_from_account_mailbox(
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

    /// Get mailbox from address
    pub async fn mailbox(
        &self,
        address: AccountId32,
        count: u32,
    ) -> Result<Vec<(StoredMessage, Interval<u32>)>> {
        let storage = self.storage().at(None).await?;
        let mut query_key =
            storage_address_root_bytes(&subxt::dynamic::storage_root("GearMessenger", "Mailbox"));
        StorageMapKey::new(&address, StorageHasher::Identity).to_bytes(&mut query_key);

        let keys = storage.fetch_keys(&query_key, count, None).await?;

        let mut mailbox: Vec<(StoredMessage, Interval<u32>)> = vec![];
        for key in keys.into_iter() {
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
