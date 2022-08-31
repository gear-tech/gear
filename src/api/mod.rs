//! Gear api
use crate::{
    api::{config::GearConfig, generated::api::RuntimeApi, signer::Signer},
    result::Result,
};
use core::ops::{Deref, DerefMut};
use subxt::{ClientBuilder, PolkadotExtrinsicParams};

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
        Ok(Self(
            ClientBuilder::new()
                .set_url(url.unwrap_or(DEFAULT_GEAR_ENDPOINT))
                .build()
                .await?
                .to_runtime_api::<RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>>(),
        ))
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
