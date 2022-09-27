//! gear api constants methods
use crate::{api::Api, result::Result};

impl Api {
    /// pallet gas constants
    ///
    /// get gas limit
    pub async fn gas_limit(&self) -> Result<u64> {
        self.constants()
            .gear_gas()
            .block_gas_limit()
            .map_err(Into::into)
    }
}
