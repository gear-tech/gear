//! gear api constants methods
use crate::{
    api::{generated::api::constants, Api},
    result::Result,
};

impl Api {
    /// pallet gas constants
    ///
    /// get gas limit
    pub fn gas_limit(&self) -> Result<u64> {
        self.constants()
            .at(&constants().gear_gas().block_gas_limit())
            .map_err(Into::into)
    }
}
