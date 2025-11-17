// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{Api, backtrace::Backtrace, config::GearConfig, result::Result};
use calls::SignerCalls;
use core::ops::Deref;
use gsigner::substrate::SubstratePair;
pub use pair_signer::PairSigner;
use rpc::SignerRpc;
use sp_core::{crypto::Ss58Codec, sr25519::Pair};
use sp_runtime::AccountId32;
use std::sync::Arc;
use storage::SignerStorage;

mod calls;
mod pair_signer;
mod rpc;
mod storage;
mod utils;

/// Signer representation that provides access to gear API.
/// Implements low-level methods such as [`run_tx`](`Inner::run_tx`)
/// and [`force_batch`](`Signer.calls()::force_batch`).
/// Other higher-level calls are provided by [`Signer::storage`],
/// [`Signer::calls`], [`Signer::rpc`].
#[derive(Clone)]
pub struct Signer {
    signer: Arc<Inner>,
    /// Calls that get or set storage.
    pub storage: SignerStorage,
    /// Calls for interaction with on-chain programs.
    pub calls: SignerCalls,
    /// Calls to fetch data from node.
    pub rpc: SignerRpc,
}

/// Implementation of low-level calls for [`Signer`].
#[derive(Clone)]
pub struct Inner {
    api: Api,
    /// Current signer.
    signer: PairSigner<GearConfig, Pair>,
    nonce: Option<u64>,
    backtrace: Backtrace,
}

impl Signer {
    /// Get backtrace of the signer.
    pub fn backtrace(&self) -> Backtrace {
        self.calls.0.backtrace.clone()
    }

    /// New signer api.
    pub fn new(api: Api, suri: &str, passwd: Option<&str>) -> Result<Self> {
        let signer = Inner {
            api,
            signer: PairSigner::new(load_sr25519_pair(suri, passwd)?),
            nonce: None,
            backtrace: Default::default(),
        };

        Ok(Self::from_inner(signer))
    }

    fn from_inner(signer: Inner) -> Self {
        let signer = Arc::new(signer);

        Self {
            storage: SignerStorage(signer.clone()),
            calls: SignerCalls(signer.clone()),
            rpc: SignerRpc(signer.clone()),
            signer,
        }
    }

    fn replace_inner(&mut self, mut inner: Inner) {
        let backtrace = self.backtrace();
        inner.backtrace = backtrace;

        let Signer {
            signer,
            storage,
            calls,
            rpc,
        } = self;

        *signer = Arc::new(inner);
        *storage = SignerStorage(signer.clone());
        *calls = SignerCalls(signer.clone());
        *rpc = SignerRpc(signer.clone());
    }

    /// Change inner signer.
    pub fn change(mut self, suri: &str, passwd: Option<&str>) -> Result<Self> {
        let signer = PairSigner::new(load_sr25519_pair(suri, passwd)?);

        self.replace_inner(Inner {
            signer,
            ..self.signer.as_ref().clone()
        });

        Ok(self)
    }

    /// Set nonce of the signer
    pub fn set_nonce(&mut self, nonce: u64) {
        self.replace_inner(Inner {
            nonce: Some(nonce),
            ..self.signer.as_ref().clone()
        });
    }
}

impl Inner {
    /// Get address of the current signer
    pub fn address(&self) -> String {
        self.account_id().to_ss58check()
    }

    /// Get address of the current signer
    pub fn account_id(&self) -> &AccountId32 {
        self.signer.account_id()
    }

    /// Get reference to inner unsigned api
    pub fn api(&self) -> &Api {
        &self.api
    }

    pub fn signer(&self) -> &PairSigner<GearConfig, Pair> {
        &self.signer
    }
}

impl From<(Api, PairSigner<GearConfig, Pair>)> for Signer {
    fn from((api, signer): (Api, PairSigner<GearConfig, Pair>)) -> Self {
        let signer = Inner {
            api,
            signer,
            nonce: None,
            backtrace: Backtrace::default(),
        };

        Self::from_inner(signer)
    }
}

const SIGNER_ALIAS: &str = "gsdk";

fn load_sr25519_pair(suri: &str, passwd: Option<&str>) -> Result<Pair> {
    let pair =
        SubstratePair::from_suri(SIGNER_ALIAS, suri, passwd).map_err(|_| Error::InvalidSecret)?;

    pair.to_sp_pair().map_err(|_| Error::InvalidSecret)
}

impl Deref for Signer {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        self.signer.as_ref()
    }
}
