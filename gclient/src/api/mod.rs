// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

pub mod calls;
pub mod error;
pub mod listener;
pub mod storage;

use crate::{node::ws::WSAddress, EventListener};
use error::*;
use gp::api::{signer::Signer, Api};

#[derive(Clone)]
pub struct GearApi(Signer);

impl GearApi {
    pub async fn init(address: WSAddress) -> Result<Self> {
        Api::new(Some(&address.url()))
            .await
            .and_then(|api| Ok(Self(api.try_signer(None)?)))
            .map_err(Into::into)
    }

    pub async fn dev() -> Result<Self> {
        Self::init(WSAddress::dev()).await
    }

    pub async fn gear() -> Result<Self> {
        Self::init(WSAddress::gear()).await
    }

    pub async fn vara() -> Result<Self> {
        Self::init(WSAddress::vara()).await
    }

    // This stuff to be considered.
    pub async fn subscribe(&self) -> Result<EventListener<'_>> {
        let events = self.0.events().await?;
        Ok(EventListener(events))
    }

    pub fn set_nonce(&mut self, nonce: u32) {
        self.0.signer.set_nonce(nonce)
    }

    pub async fn rpc_nonce(&self) -> Result<u32> {
        self.0
            .client
            .rpc()
            .system_account_next_index(self.0.signer.account_id())
            .await
            .map_err(Into::into)
    }
}
