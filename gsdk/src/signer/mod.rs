//! Gear api with signer
use crate::{
    config::GearConfig,
    result::{Error, Result},
    Api,
};
use std::ops::{Deref, DerefMut};
use subxt::tx::PairSigner;

use sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT};
use sp_runtime::AccountId32;

mod calls;
mod rpc;
mod utils;

#[derive(Clone)]
pub struct Signer {
    api: Api,
    /// Current signer.
    pub signer: PairSigner<GearConfig, Pair>,
    nonce: Option<u32>,
}

impl Signer {
    /// New signer api.
    pub fn new(api: Api, suri: &str, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api,
            signer: PairSigner::new(
                Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?,
            ),
            nonce: None,
        })
    }

    /// Change inner signer.
    pub fn change(self, suri: &str, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api: self.api,
            signer: PairSigner::new(
                Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?,
            ),
            nonce: None,
        })
    }

    pub fn set_nonce(&mut self, nonce: u32) {
        self.nonce = Some(nonce)
    }

    /// Get address of the current signer
    pub fn address(&self) -> String {
        self.signer.account_id().to_ss58check()
    }

    /// Get address of the current signer
    pub fn account_id(&self) -> &AccountId32 {
        self.signer.account_id()
    }
}

impl From<(Api, PairSigner<GearConfig, Pair>)> for Signer {
    fn from((api, signer): (Api, PairSigner<GearConfig, Pair>)) -> Self {
        Signer {
            api,
            signer,
            nonce: None,
        }
    }
}

impl Deref for Signer {
    type Target = Api;

    fn deref(&self) -> &Self::Target {
        &self.api
    }
}

impl DerefMut for Signer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.api
    }
}
