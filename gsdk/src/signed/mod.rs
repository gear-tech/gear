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

pub use self::{pair_signer::PairSigner, utils::TxEvents};

use crate::{Api, backtrace::Backtrace, config::GearConfig, result::Result};
use sp_core::{Pair as PairT, sr25519::Pair};
use sp_runtime::AccountId32;
use std::sync::Arc;

mod calls;
mod pair_signer;
mod rpc;
mod storage;
mod utils;

pub type Signer = PairSigner<GearConfig, Pair>;

/// Signed Gear API wrapper.
#[derive(derive_more::Debug, Clone, derive_more::Into, derive_more::AsRef, derive_more::Deref)]
pub struct SignedApi {
    #[into]
    #[as_ref]
    #[deref]
    api: Api,

    /// Current signer.
    #[debug("<signer>")]
    signer: Arc<PairSigner<GearConfig, Pair>>,

    nonce: Option<u64>,
    backtrace: Backtrace,
}

impl Api {
    /// Attachs a signer to the API.
    pub fn signed(self, suri: &str, passwd: Option<&str>) -> Result<SignedApi> {
        SignedApi::new(self, suri, passwd)
    }
}

impl SignedApi {
    /// Constructs new signed API.
    pub fn new(api: Api, suri: &str, passwd: Option<&str>) -> Result<Self> {
        Ok(Self {
            api,
            signer: PairSigner::new(Pair::from_string(suri, passwd)?).into(),
            nonce: None,
            backtrace: Backtrace::default(),
        })
    }

    /// Returns a reference to the inner unsigned API wrapper.
    pub fn unsigned(&self) -> &Api {
        &self.api
    }

    /// Returns a reference to the inner signer.
    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    /// Returns the address of the current signer.
    pub fn account_id(&self) -> &AccountId32 {
        self.signer.account_id()
    }

    /// Returns the backtrace of the signed API.
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }

    /// Sets nonce of the signer.
    pub fn set_nonce(&mut self, nonce: u64) {
        self.nonce = Some(nonce);
    }
}
