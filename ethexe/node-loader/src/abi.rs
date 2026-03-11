use alloy::{primitives::Address, sol};
use anyhow::Result;
use ethexe_ethereum::Ethereum;

sol!(
    #[sol(rpc)]
    BatchMulticall,
    "../ethereum/abi/BatchMulticall.json"
);

pub async fn deploy_send_message_multicall(api: &Ethereum) -> Result<Address> {
    let multicall = BatchMulticall::deploy(api.provider()).await?;

    Ok(*multicall.address())
}
