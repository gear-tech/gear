//! gear storage apis
use crate::{
    api::{
        generated::api::runtime_types::{frame_system::AccountInfo, pallet_balances::AccountData},
        Api,
    },
    result::Result,
};
use subxt::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

impl Api {
    /// Get account info by address
    pub async fn info(&self, address: &str) -> Result<AccountInfo<u32, AccountData<u128>>> {
        Ok(self
            .storage()
            .system()
            .account(&AccountId32::from_ss58check(address)?, None)
            .await?)
    }

    /// Get balance by account address
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self
            .storage()
            .system()
            .account(&AccountId32::from_ss58check(address)?, None)
            .await?
            .data
            .free)
    }
}

mod system {
    use crate::{api::Api, result::Result};

    impl Api {
        pub async fn number(&self) -> Result<u32> {
            self.storage()
                .system()
                .number(None)
                .await
                .map_err(Into::into)
        }
    }
}

mod gear {
    use crate::{
        api::{
            generated::api::{
                gear_messenger,
                runtime_types::{
                    gear_common::{storage::primitives::Interval, ActiveProgram},
                    gear_core::{
                        code::InstrumentedCode, ids::CodeId, memory::PageNumber,
                        message::stored::StoredMessage,
                    },
                },
            },
            types, utils, Api,
        },
        result::{Error, Result},
    };
    use hex::ToHex;
    use parity_scale_codec::Decode;
    use std::collections::HashMap;
    use subxt::{
        sp_core::{storage::StorageKey, H256},
        sp_runtime::AccountId32,
        storage::StorageKeyPrefix,
        StorageEntryKey, StorageMapKey,
    };

    impl Api {
        /// Get `InstrumentedCode` by `code_hash`
        pub async fn code_storage(&self, code_hash: [u8; 32]) -> Result<Option<InstrumentedCode>> {
            Ok(self
                .storage()
                .gear_program()
                .code_storage(&CodeId(code_hash), None)
                .await?)
        }

        /// Get active program from program id.
        pub async fn gprog(&self, pid: H256) -> Result<ActiveProgram> {
            let bytes = self
                .client
                .storage()
                .fetch_raw(StorageKey(utils::program_key(pid)), None)
                .await?
                .ok_or_else(|| Error::ProgramNotFound(pid.encode_hex()))?;

            match types::Program::decode(&mut bytes.0.as_ref())? {
                types::Program::Active(p) => Ok(p),
                types::Program::Terminated => Err(Error::ProgramTerminated),
            }
        }

        /// Get pages of active program.
        pub async fn gpages(&self, pid: H256, program: ActiveProgram) -> Result<types::GearPages> {
            let mut pages = HashMap::new();
            for page in program.pages_with_data {
                let value = self
                    .client
                    .storage()
                    .fetch_raw(StorageKey(utils::page_key(pid, PageNumber(page.0))), None)
                    .await?
                    .ok_or_else(|| Error::PageNotFound(page.0, pid.encode_hex()))?;
                pages.insert(page.0, value.0);
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
            let prefix = StorageKeyPrefix::new::<gear_messenger::storage::Mailbox>();
            let entry_key = StorageEntryKey::Map(vec![StorageMapKey::new(
                &address,
                subxt::StorageHasher::Identity,
            )]);

            let query_key = entry_key.final_key(prefix);
            let keys = self
                .client
                .rpc()
                .storage_keys_paged(Some(query_key), count, None, None)
                .await?;

            let mut mailbox: Vec<(StoredMessage, Interval<u32>)> = vec![];
            for key in keys.into_iter() {
                if let Some(storage_data) = self.client.storage().fetch_raw(key, None).await? {
                    if let Ok(value) =
                        <(StoredMessage, Interval<u32>)>::decode(&mut &storage_data.0[..])
                    {
                        mailbox.push(value);
                    }
                }
            }

            Ok(mailbox)
        }
    }
}
