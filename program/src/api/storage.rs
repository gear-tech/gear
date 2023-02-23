//! gear storage apis
use crate::{
    api::{
        generated::api::runtime_types::{
            frame_system::AccountInfo,
            gear_common::{storage::primitives::Interval, ActiveProgram, Program},
            gear_core::{code::InstrumentedCode, message::stored::StoredMessage},
            pallet_balances::AccountData,
        },
        types, Api,
    },
    result::{ClientError, Result},
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
    async fn fetch_storage<'a, Address, Value>(&self, address: &'a Address) -> Result<Value>
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
                .ok_or(ClientError::StorageNotFound)?
                .into_encoded()
                .as_ref(),
        )?)
    }

    ////
    // frame-system
    ////

    /// Get account info by address
    pub async fn info(&self, address: &str) -> Result<AccountInfo<u32, AccountData<u128>>> {
        let dest = AccountId32::from_ss58check(address)?;
        let addr = subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(dest)]);

        Ok(self.fetch_storage(&addr).await?)
    }

    /// Get block number.
    pub async fn number(&self) -> Result<u32> {
        let addr = subxt::dynamic::storage_root("System", "Number");
        Ok(self.fetch_storage(&addr).await?)
    }

    /// Get balance by account address
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }

    ////
    // pallet-session
    ////

    /// Get all validators from pallet_session.
    pub async fn validators(&self) -> Result<Vec<AccountId32>> {
        let addr = subxt::dynamic::storage_root("Session", "Validators");
        Ok(self.fetch_storage(&addr).await?)
    }

    ////
    // pallet-gear
    ////

    /// Get `InstrumentedCode` by `code_hash`
    pub async fn code_storage(&self, code_hash: [u8; 32]) -> Result<InstrumentedCode> {
        let addr = subxt::dynamic::storage(
            "GearProgram",
            "CodeStorage",
            vec![Value::from_bytes(code_hash)],
        );
        Ok(self.fetch_storage(&addr).await?)
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
            _ => Err(ClientError::ProgramTerminated.into()),
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
                .ok_or_else(|| ClientError::PageNotFound(page.0, pid.encode_hex()))?;
            let decoded = <[u8; GEAR_PAGE_SIZE]>::decode(&mut &encoded_page[..])?;
            pages.insert(page.0, decoded.to_vec());
        }

        Ok(pages)
    }

    /// Get program pages from program id.
    pub async fn program_pages(&self, pid: H256) -> Result<types::GearPages> {
        self.gpages(pid, self.gprog(pid).await?).await
    }

    /// Get mailbox of address
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
