//! gear api constants methods
use crate::{
    api::{generated::api::gas, Api},
    Result,
};

impl Api {
    /// pallet gas constants
    ///
    /// get gas limit
    pub async fn gas_limit(&self) -> Result<u64> {
        gas::constants::ConstantsApi::new(&self.runtime.client)
            .block_gas_limit()
            .map_err(Into::into)
    }
}
