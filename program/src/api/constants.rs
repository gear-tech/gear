//! gear api constants methods
use crate::{api::Api, result::Result};

impl Api {
    /// pallet gas constants
    ///
    /// get gas limit
    pub fn gas_limit(&self) -> Result<u64> {
        self.constants()
            .gear_gas()
            .block_gas_limit()
            .map_err(Into::into)
    }

    /// pallet babe constant
    ///
    /// get expected block time
    pub fn expected_block_time(&self) -> Result<u64> {
        self.constants()
            .babe()
            .expected_block_time()
            .map_err(Into::into)
    }
}
