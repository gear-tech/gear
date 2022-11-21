//! Gear api with signer
use crate::{
    api::{config::GearConfig, Api},
    keystore,
    result::{Error, Result},
};
use std::ops::{Deref, DerefMut};
use subxt::{
    ext::{
        sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT},
        sp_runtime::AccountId32,
    },
    tx::PairSigner,
};

mod calls;
mod rpc;
mod utils;

#[derive(Clone)]
pub struct Signer {
    api: Api,
    /// Current signer.
    pub signer: PairSigner<GearConfig, Pair>,
}

impl Signer {
    /// New signer api.
    pub fn new(api: Api, suri: &str, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api,
            signer: PairSigner::new(
                Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?,
            ),
        })
    }

    /// Change inner signer.
    pub fn change(self, suri: &str, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api: self.api,
            signer: PairSigner::new(
                Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?,
            ),
        })
    }

    /// New signer from cache
    pub fn cache(api: Api, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api,
            signer: keystore::cache(passwd)?,
        })
    }

    /// New signer from keyring
    pub fn keyring(api: Api, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api,
            signer: keystore::keyring(passwd)?,
        })
    }

    /// Try new signer from keyring or cache.
    pub fn try_new(api: Api, passwd: Option<&str>) -> Result<Self> {
        if let Ok(s) = Self::cache(api.clone(), passwd) {
            Ok(s)
        } else {
            Self::keyring(api, passwd)
        }
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
