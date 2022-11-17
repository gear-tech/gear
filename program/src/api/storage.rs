//! gear storage apis
use crate::{
    api::{
        generated::api::{
            runtime_types::{frame_system::AccountInfo, pallet_balances::AccountData},
            storage,
        },
        Api,
    },
    result::{ClientError, Result},
};
use subxt::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

impl Api {
    /// Get account info by address
    pub async fn info(&self, address: &str) -> Result<AccountInfo<u32, AccountData<u128>>> {
        let at = storage()
            .system()
            .account(AccountId32::from_ss58check(address)?);
        Ok(self
            .storage()
            .fetch(&at, None)
            .await?
            .ok_or(ClientError::StorageNotFound)?)
    }

    /// Get balance by account address
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self.info(address).await?.data.free)
    }
}

mod system {
    use crate::{
        api::{generated::api::storage, Api},
        result::{ClientError, Result},
    };

    impl Api {
        pub async fn number(&self) -> Result<u32> {
            let at = storage().system().number();
            Ok(self
                .storage()
                .fetch(&at, None)
                .await?
                .ok_or(ClientError::StorageNotFound)?)
        }
    }
}

mod gear {
    use crate::{
        api::{
            generated::api::{
                runtime_types::{
                    gear_common::{storage::primitives::Interval, ActiveProgram},
                    gear_core::{
                        code::InstrumentedCode, ids::CodeId, memory::PageNumber,
                        message::stored::StoredMessage,
                    },
                },
                storage,
            },
            types, utils, Api,
        },
        result::{ClientError, Result},
    };
    use hex::ToHex;
    use parity_scale_codec::Decode;
    use std::collections::HashMap;
    use subxt::{
        ext::{sp_core::H256, sp_runtime::AccountId32},
        storage::address::{StorageHasher, StorageMapKey},
    };

    impl Api {
        /// Get `InstrumentedCode` by `code_hash`
        pub async fn code_storage(&self, code_hash: [u8; 32]) -> Result<Option<InstrumentedCode>> {
            let at = storage().gear_program().code_storage(&CodeId(code_hash));

            Ok(self.storage().fetch(&at, None).await?)
        }

        /// Get active program from program id.
        pub async fn gprog(&self, pid: H256) -> Result<ActiveProgram> {
            let bytes = self
                .storage()
                .fetch_raw(&utils::program_key(pid), None)
                .await?
                .ok_or_else(|| ClientError::ProgramNotFound(pid.encode_hex()))?;

            match types::Program::decode(&mut bytes.as_ref())? {
                types::Program::Active(p) => Ok(p),
                types::Program::Terminated => Err(ClientError::ProgramTerminated.into()),
            }
        }

        /// Get pages of active program.
        pub async fn gpages(&self, pid: H256, program: ActiveProgram) -> Result<types::GearPages> {
            let mut pages = HashMap::new();
            for page in program.pages_with_data {
                let value = self
                    .storage()
                    .fetch_raw(&utils::page_key(pid, PageNumber(page.0)), None)
                    .await?
                    .ok_or_else(|| ClientError::PageNotFound(page.0, pid.encode_hex()))?;
                pages.insert(page.0, value);
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
            let mut query_key = storage().gear_messenger().mailbox_root().to_root_bytes();
            StorageMapKey::new(&address, StorageHasher::Identity).to_bytes(&mut query_key);

            let keys = self
                .storage()
                .fetch_keys(&query_key, count, None, None)
                .await?;

            let mut mailbox: Vec<(StoredMessage, Interval<u32>)> = vec![];
            for key in keys.into_iter() {
                if let Some(storage_data) = self.storage().fetch_raw(&key.0, None).await? {
                    if let Ok(value) =
                        <(StoredMessage, Interval<u32>)>::decode(&mut &storage_data[..])
                    {
                        mailbox.push(value);
                    }
                }
            }

            Ok(mailbox)
        }
    }
}
