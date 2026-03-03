use alloy::primitives::Address;
use anyhow::Result;
use ethexe_ethereum::Ethereum;

alloy::sol!(
    #[sol(rpc)]
    BatchMulticall,
    "BatchMulticall.json"
);

pub async fn deploy_send_message_multicall(api: &Ethereum) -> Result<Address> {
    let multicall = BatchMulticall::deploy(api.provider()).await?;

    Ok(*multicall.address())
}
