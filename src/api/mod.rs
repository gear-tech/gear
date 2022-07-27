//! gear api
use crate::{
    api::{config::GearConfig, generated::api::RuntimeApi},
    keystore, Result,
};
use subxt::{sp_core::sr25519::Pair, ClientBuilder, PairSigner, PolkadotExtrinsicParams};

mod calls;
pub mod config;
mod constants;
pub mod events;
pub mod generated;
mod rpc;
mod storage;
pub mod types;
mod utils;

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc-node.gear-tech.io:443";

/// gear api
pub struct Api {
    runtime: RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>,
    /// Current signer.
    pub signer: PairSigner<GearConfig, Pair>,
}

impl Api {
    /// New gear api
    pub async fn new(url: Option<&str>, passwd: Option<&str>) -> Result<Self> {
        let runtime = ClientBuilder::new()
            .set_url(url.unwrap_or(DEFAULT_GEAR_ENDPOINT))
            .build()
            .await?
            .to_runtime_api::<RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>>();

        let signer = keystore::cache(passwd.as_ref().copied())?;

        Ok(Self { runtime, signer })
    }
}
