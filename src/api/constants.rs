//! gear api constants methods
use crate::{api::Api, Result};

impl Api {
    /// pallet gas constants
    ///
    /// get gas limit
    pub async fn gas_limit(&self) -> Result<u64> {
        self.runtime
            .constants()
            .gear_gas()
            .block_gas_limit()
            .map_err(Into::into)
    }
}
