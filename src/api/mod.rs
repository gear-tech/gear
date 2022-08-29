//! gear api
use crate::{
    api::{config::GearConfig, generated::api::RuntimeApi},
    keystore,
    result::{Error, Result},
};
use std::{cell::RefCell, sync::Arc};
use subxt::{
    sp_core::{sr25519::Pair, Pair as PairT},
    ClientBuilder, PairSigner, PolkadotExtrinsicParams,
};

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
    /// Balance tracker
    pub balance: Arc<RefCell<u128>>,
}

impl Api {
    /// Build runtime api
    pub async fn build_runtime_api(
        url: Option<&str>,
    ) -> Result<RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>> {
        Ok(ClientBuilder::new()
            .set_url(url.unwrap_or(DEFAULT_GEAR_ENDPOINT))
            .build()
            .await?
            .to_runtime_api::<RuntimeApi<GearConfig, PolkadotExtrinsicParams<GearConfig>>>())
    }

    /// New gear api
    pub async fn new(url: Option<&str>, passwd: Option<&str>) -> Result<Self> {
        let runtime = Self::build_runtime_api(url).await?;
        let signer = keystore::cache(passwd.as_ref().copied())?;
        let api = Self {
            runtime,
            signer,
            balance: Default::default(),
        };

        api.update_balance().await?;
        Ok(api)
    }

    /// New api with secret uri
    pub async fn new_with_suri(
        url: Option<&str>,
        suri: &str,
        passwd: Option<&str>,
    ) -> Result<Self> {
        let runtime = Self::build_runtime_api(url).await?;
        let signer =
            PairSigner::new(Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?);

        Ok(Self {
            runtime,
            signer,
            balance: Default::default(),
        })
    }
}
