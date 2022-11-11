//! Gear api
use crate::result::Result;
use config::GearConfig;
use core::ops::{Deref, DerefMut};
use rpc::RpcClient;
use signer::Signer;
use subxt::OnlineClient;

pub mod config;
mod constants;
pub mod events;
pub mod generated;
mod rpc;
pub mod signer;
mod storage;
pub mod types;
mod utils;

/// Gear api wrapper.
#[derive(Clone)]
pub struct Api(OnlineClient<GearConfig>);

impl Api {
    /// Create new API client.
    pub async fn new(url: Option<&str>) -> Result<Self> {
        Self::new_with_timeout(url, None).await
    }

    /// Create new API client with timeout.
    pub async fn new_with_timeout(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        Ok(Self(
            OnlineClient::from_rpc_client(RpcClient::new(url, timeout).await?).await?,
        ))
    }

    /// Subscribe all events
    pub async fn events(&self) -> Result<types::Events> {
        Ok(self.0.events().subscribe().await?)
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
    type Target = OnlineClient<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Api {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
