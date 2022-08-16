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
            .runtime
            .storage()
            .system()
            .account(&AccountId32::from_ss58check(address)?, None)
            .await?)
    }

    /// Get balance by account address
    pub async fn get_balance(&self, address: &str) -> Result<u128> {
        Ok(self
            .runtime
            .storage()
            .system()
            .account(&AccountId32::from_ss58check(address)?, None)
            .await?
            .data
            .free)
    }
}

mod gear {
    use crate::{
        api::{
            generated::api::runtime_types::{
                gear_common::ActiveProgram,
                gear_core::{code::InstrumentedCode, ids::CodeId, memory::PageNumber},
            },
            types, utils, Api,
        },
        result::{Error, Result},
    };
    use hex::ToHex;
    use parity_scale_codec::Decode;
    use std::collections::HashMap;
    use subxt::sp_core::{storage::StorageKey, H256};

    impl Api {
        pub async fn code_storage(&self, code_hash: [u8; 32]) -> Result<Option<InstrumentedCode>> {
            Ok(self
                .runtime
                .storage()
                .gear_program()
                .code_storage(&CodeId(code_hash), None)
                .await?)
        }

        /// Get active program from program id.
        pub async fn gprog(&self, pid: H256) -> Result<ActiveProgram> {
            let bytes = self
                .runtime
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
                    .runtime
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
    }
}
