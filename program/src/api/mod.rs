//! Gear api
use crate::{
    api::{config::GearConfig, generated::api::RuntimeApi, signer::Signer},
    result::Result,
};
use core::ops::{Deref, DerefMut};
use std::{str::FromStr, time::Duration};
use subxt::{
    rpc::{RpcClientBuilder, Uri, WsTransportClientBuilder},
    ClientBuilder, PolkadotExtrinsicParams,
};

pub mod config;
mod constants;
pub mod events;
pub mod generated;
pub mod signer;
mod storage;
pub mod types;
mod utils;

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc-node.gear-tech.io:443";

/// gear api
#[derive(Clone)]
pub struct Api(RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>);

impl Api {
    /// Build runtime api
    pub async fn new(url: Option<&str>) -> Result<Self> {
        Self::new_with_timeout(url, None).await
    }

    /// Build runtime api with timeout
    ///
    /// # TODO
    ///
    /// run [subscribe_to_updates](https://docs.rs/subxt/latest/subxt/client/struct.OnlineClient.html#method.subscribe_to_updates)
    /// after #1629 since
    ///
    /// * the build may include both `gear` and `vara` features.
    /// * users may have installed this CLI tool independently and the metadata is outdated.
    pub async fn new_with_timeout(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        let (tx, rx) = WsTransportClientBuilder::default()
            .connection_timeout(Duration::from_millis(timeout.unwrap_or(60_000)))
            .build(Uri::from_str(url.unwrap_or(DEFAULT_GEAR_ENDPOINT))?)
            .await?;

        let rpc = RpcClientBuilder::default().build_with_tokio(tx, rx);
        let builder = ClientBuilder::new().set_client(rpc);

        Ok(Self(builder.build().await?.to_runtime_api::<RuntimeApi<
            GearConfig,
            PolkadotExtrinsicParams<GearConfig>,
        >>()))
    }

    /// New signer from api
    pub fn signer(self, suri: &str, passwd: Option<&str>) -> Result<Signer> {
        Signer::new(self, suri, passwd)
    }

    /// Try new signer from api
    pub fn try_signer(self, passwd: Option<&str>) -> Result<Signer> {
        Signer::try_new(self, passwd)
    }
}

impl Deref for Api {
    type Target = RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Api {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
