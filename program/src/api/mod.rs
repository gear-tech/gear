//! Gear api
use crate::result::Result;
use client::RpcClient;
use config::GearConfig;
use core::ops::{Deref, DerefMut};
use signer::Signer;
use std::sync::Arc;
use subxt::OnlineClient;

mod client;
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
pub struct Api {
    /// How many times we'll retry when rpc requests failed.
    pub retry: u16,
    client: OnlineClient<GearConfig>,
}

impl Api {
    /// Create new API client.
    pub async fn new(url: Option<&str>) -> Result<Self> {
        Self::new_with_timeout(url, None).await
    }

    /// Create new API client with timeout.
    pub async fn new_with_timeout(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        Ok(Self {
            retry: 5,
            client: OnlineClient::from_rpc_client(Arc::new(RpcClient::new(url, timeout).await?))
                .await?,
        })
    }

    /// Setup retry times and return the API instance.
    pub fn with_retry(mut self, retry: u16) -> Self {
        self.retry = retry;
        self
    }

    /// Subscribe all blocks
    pub async fn blocks(&self) -> Result<types::Blocks> {
        Ok(types::Blocks(self.client.blocks().subscribe_all().await?))
    }

    /// Subscribe finalized blocks
    pub async fn finalized_blocks(&self) -> Result<types::Blocks> {
        Ok(types::Blocks(
            self.client.blocks().subscribe_finalized().await?,
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
    type Target = OnlineClient<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for Api {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}
