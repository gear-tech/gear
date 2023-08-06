// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use core::ops::Deref;
pub use pair_signer::PairSigner;
use sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT};
use sp_runtime::AccountId32;
use std::sync::Arc;

mod calls;
mod pair_signer;
mod rpc;
mod utils;

/// Signer representation that provides access to gear API.
/// Implements low-level methods such as [`run_tx`](`SignerInner::run_tx`)
/// and [`force_batch`](`SignerInner::force_batch`).
/// Other higher-level calls are provided by [`Signer::sudo`],
/// [`Signer::balance`], [`Signer::calls`], [`Signer::rpc`].
#[derive(Clone)]
pub struct Signer {
    signer: Arc<SignerInner>,
    /// Calls that require sudo.
    pub sudo: SignerSudo,
    /// Calls to interact with account balance.
    pub balance: SignerBalance,
    /// Calls for interaction with on-chain programs.
    pub calls: SignerCalls,
    /// Calls to fetch data from node.
    pub rpc: SignerRpc,
}

#[derive(Clone)]
pub struct SignerSudo(Arc<SignerInner>);

#[derive(Clone)]
pub struct SignerBalance(Arc<SignerInner>);

#[derive(Clone)]
pub struct SignerCalls(Arc<SignerInner>);

#[derive(Clone)]
pub struct SignerRpc(Arc<SignerInner>);

#[derive(Clone)]
pub struct SignerInner {
    api: Api,
    /// Current signer.
    signer: PairSigner<GearConfig, Pair>,
    nonce: Option<u32>,
}

impl Signer {
    /// New signer api.
    pub fn new(api: Api, suri: &str, passwd: Option<&str>) -> Result<Self> {
        let signer = SignerInner {
            api,
            signer: PairSigner::new(
                Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?,
            ),
            nonce: None,
        };

        Ok(Self::from_inner(signer))
    }

    fn from_inner(signer: SignerInner) -> Self {
        let signer = Arc::new(signer);

        Self {
            sudo: SignerSudo(signer.clone()),
            balance: SignerBalance(signer.clone()),
            calls: SignerCalls(signer.clone()),
            rpc: SignerRpc(signer.clone()),
            signer,
        }
    }

    #[deny(unused_variables)]
    fn replace_inner(&mut self, inner: SignerInner) {
        let Signer {
            signer,
            sudo,
            balance,
            calls,
            rpc,
        } = self;

        *signer = Arc::new(inner);
        *sudo = SignerSudo(signer.clone());
        *balance = SignerBalance(signer.clone());
        *calls = SignerCalls(signer.clone());
        *rpc = SignerRpc(signer.clone());
    }

    /// Change inner signer.
    pub fn change(mut self, suri: &str, passwd: Option<&str>) -> Result<Self> {
        let signer =
            PairSigner::new(Pair::from_string(suri, passwd).map_err(|_| Error::InvalidSecret)?);

        self.replace_inner(SignerInner {
            signer,
            ..self.signer.as_ref().clone()
        });

        Ok(self)
    }

    /// Set nonce of the signer
    pub fn set_nonce(&mut self, nonce: u32) {
        self.replace_inner(SignerInner {
            nonce: Some(nonce),
            ..self.signer.as_ref().clone()
        });
    }
}

impl SignerInner {
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
}

impl From<(Api, PairSigner<GearConfig, Pair>)> for Signer {
    fn from((api, signer): (Api, PairSigner<GearConfig, Pair>)) -> Self {
        let signer = SignerInner {
            api,
            signer,
            nonce: None,
        };

        Self::from_inner(signer)
    }
}

impl Deref for Signer {
    type Target = SignerInner;

    fn deref(&self) -> &SignerInner {
        self.signer.as_ref()
    }
}
