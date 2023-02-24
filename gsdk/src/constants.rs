//! gear api constants methods
use crate::{result::Result, Api};
use parity_scale_codec::Decode;

impl Api {
    /// pallet gas constants
    ///
    /// Get gas limit.
    pub fn gas_limit(&self) -> Result<u64> {
        let addr = subxt::dynamic::constant("GearGas", "BlockGasLimit");
        Ok(u64::decode(
            &mut self.constants().at(&addr)?.into_encoded().as_ref(),
        )?)
    }

    /// pallet babe constant
    ///
    /// Get expected block time.
    pub fn expected_block_time(&self) -> Result<u64> {
        let addr = subxt::dynamic::constant("Babe", "ExpectedBlockTime");
        Ok(u64::decode(
            &mut self.constants().at(&addr)?.into_encoded().as_ref(),
        )?)
    }
}
