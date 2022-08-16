//! gear api rpc methods
use crate::{
    api::{types, Api},
    result::Result,
};

use std::sync::Arc;
use subxt::{
    rpc::{rpc_params, ClientT},
    sp_core::{Bytes, H256},
    RpcClient,
};

impl Api {
    /// get rpc client
    pub fn rpc(&self) -> Arc<RpcClient> {
        self.runtime.client.rpc().client.clone()
    }

    /// public key of the signer in H256
    pub fn source(&self) -> H256 {
        AsRef::<[u8; 32]>::as_ref(self.signer.account_id()).into()
    }

    /// gear_getInitGasSpent
    pub async fn get_init_gas_spent(
        &self,
        code: Bytes,
        payload: Bytes,
        value: u64,
        at: Option<H256>,
    ) -> Result<types::GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitGas",
                rpc_params![self.source(), code, payload, value, true, at],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_getHandleGasSpent
    #[allow(dead_code)]
    pub async fn get_handle_gas_spent(
        &self,
        dest: H256,
        payload: Bytes,
        value: u128,
        at: Option<H256>,
    ) -> Result<types::GasInfo> {
        self.rpc()
            .request(
                "gear_calculateHandleGas",
                rpc_params![self.source(), dest, payload, value, true, at],
            )
            .await
            .map_err(Into::into)
    }
}
