//! gear api utils
use crate::{
    api::{generated::api::runtime_types::sp_runtime::DispatchError, Api},
    result::Result,
};
use parity_scale_codec::Encode;
use subxt::error::{DispatchError as SubxtDispatchError, Error, ModuleError, ModuleErrorData};

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

    /// Decode `DispatchError` to `subxt::error::Error`.
    pub fn decode_error(&self, dispatch_error: DispatchError) -> Error {
        if let DispatchError::Module(ref err) = dispatch_error {
            if let Ok(error_details) = self.metadata().error(err.index, err.error[0]) {
                return SubxtDispatchError::Module(ModuleError {
                    pallet: error_details.pallet().to_string(),
                    error: error_details.error().to_string(),
                    description: error_details.docs().to_vec(),
                    error_data: ModuleErrorData {
                        pallet_index: err.index,
                        error: err.error,
                    },
                })
                .into();
            }
        }

        SubxtDispatchError::Other(dispatch_error.encode()).into()
    }
}
