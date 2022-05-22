//! gear api utils
use crate::{api::Api, Result};
use std::future::Future;
use subxt::rpc::NumberOrHex;

impl Api {
    /// estimate gas
    pub async fn estimate_gas<F, Fut>(&self, gas: u64, estimate: F) -> Result<u64>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<NumberOrHex>>,
    {
        self.cmp_gas_limit(if gas != 0 {
            gas
        } else {
            if let NumberOrHex::Number(n) = estimate().await? {
                n
            } else {
                0
            }
        })
        .await
    }

    /// compare gas limit
    pub async fn cmp_gas_limit(&self, gas: u64) -> Result<u64> {
        // FIXME
        //
        // the current staging testnet doesn't have this api
        if let Ok(limit) = self.gas_limit().await {
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
