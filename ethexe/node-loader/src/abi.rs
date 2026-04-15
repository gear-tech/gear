//! Solidity bindings and helpers used by the loader.

use alloy::{primitives::Address, sol};
use anyhow::Result;
use ethexe_ethereum::Ethereum;

sol!(
    #[sol(rpc)]
    BatchMulticall,
    "../ethereum/abi/BatchMulticall.json"
);

/// Deploys the `BatchMulticall` helper contract and returns its address.
///
/// Load mode uses this contract to amortize `send_message` and
/// `create_program` traffic over fewer Ethereum transactions.
pub async fn deploy_send_message_multicall(api: &Ethereum) -> Result<Address> {
    let multicall = BatchMulticall::deploy(api.provider()).await?;

    Ok(*multicall.address())
}
