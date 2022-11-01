//! gear api utils
use crate::{
    api::{generated::api::runtime_types::gear_core::memory::PageNumber, Api},
    result::Result,
};
use parity_scale_codec::Encode;
use std::mem;
use subxt::ext::sp_core::H256;

const STORAGE_PROGRAM_PREFIX: &[u8] = b"g::prog::";
const STORAGE_PROGRAM_PAGES_PREFIX: &[u8] = b"g::pages::";

pub fn program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn page_key(id: H256, page: PageNumber) -> Vec<u8> {
    let id_bytes = id.as_fixed_bytes();
    let mut key = Vec::with_capacity(
        STORAGE_PROGRAM_PAGES_PREFIX.len() + id_bytes.len() + 2 + mem::size_of::<u32>(),
    );
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    key.extend(id.as_fixed_bytes());
    key.extend(b"::");
    key.extend(page.0.to_le_bytes());

    key
}

impl Api {
    /// compare gas limit
    pub fn cmp_gas_limit(&self, gas: u64) -> Result<u64> {
        if let Ok(limit) = self.gas_limit() {
            Ok(if gas > limit {
                log::warn!("gas limit too high, use {} from the chain config", limit);
                limit
            } else {
                gas
            })
        } else {
            Ok(gas)
        }
    }
}
