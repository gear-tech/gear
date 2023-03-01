// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Gear api with signer
use crate::{
    config::GearConfig,
    result::{Error, Result},
    Api,
};
pub use pair_signer::PairSigner;
use sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT};
use sp_runtime::AccountId32;

mod calls;
mod pair_signer;
mod rpc;
mod utils;

#[derive(Clone)]
pub struct Signer {
    api: Api,
    /// Current signer.
    signer: PairSigner<GearConfig, Pair>,
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

    /// Get reference to inner unsigned api
    pub fn api(&self) -> &Api {
        &self.api
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
