// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

use crate::{
    client::{Backend, Client, Code, Message, Program, TxResult, ALICE},
    GearApi,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use gear_core::{ids::ProgramId, message::UserStoredMessage};
use gprimitives::{ActorId, MessageId};
use gsdk::{
    ext::sp_core::{sr25519, Pair},
    metadata::runtime_types::gear_common::storage::primitives::Interval,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, MutexGuard};

/// GClient instance
#[derive(Clone)]
pub struct GClient {
    inner: Arc<Mutex<GearApi>>,
    pairs: HashMap<ActorId, String>,
}

impl GClient {
    /// New gclient instance
    pub async fn new(address: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(GearApi::init(address.as_ref().parse()?).await?)),
            pairs: HashMap::from_iter(vec![(ALICE, "//Alice".to_string())].into_iter()),
        })
    }

    /// New general client with GClient as backend
    pub async fn client() -> Result<Client<Self>> {
        Ok(Client::<GClient> {
            backend: GClient::from(GearApi::dev().await?),
        })
    }

    /// Add sr25519 pair to gclient
    ///
    /// NOTE: the suri of the pairs will be stored in memory, use this method
    /// at your own risk! If you have better ideas to optimize this method, PRs
    /// are welcome!
    pub fn add_pair(&mut self, suri: impl AsRef<str>) -> Result<()> {
        let mut patt = suri.as_ref().splitn(2, ':');
        let pair = sr25519::Pair::from_string(
            patt.next()
                .ok_or(anyhow!("Invalid suri, failed to add pair"))?,
            patt.next(),
        )?;
        self.pairs
            .insert(pair.public().0.into(), suri.as_ref().to_string());

        Ok(())
    }

    /// Switch to the provided pair
    async fn switch_pair(&self, address: ActorId) -> Result<()> {
        let pair = self
            .pairs
            .get(&address)
            .ok_or(anyhow!("Could not find pair {address}"))?;

        self.inner.lock().await.change_signer(pair)?;
        Ok(())
    }

    /// Get [`GearApi`]
    async fn api(&self) -> MutexGuard<'_, GearApi> {
        self.inner.lock().await
    }
}

#[async_trait]
impl Backend for GClient {
    async fn program(&self, id: ProgramId) -> Result<Program<Self>> {
        let _ = self.inner.lock().await.program_at(id, None).await?;

        Ok(Program {
            id,
            backend: self.clone(),
        })
    }

    async fn deploy<M>(&self, code: impl Code, message: M) -> Result<TxResult<Program<Self>>>
    where
        M: Into<Message> + Send,
    {
        let wasm = code.wasm()?;
        let message = message.into();
        self.switch_pair(message.signer).await?;

        let api = self.api().await;
        let (_, id, _) = api
            .upload_program_bytes(
                wasm,
                message.salt,
                message.payload,
                message.gas_limit.unwrap_or(api.block_gas_limit()?),
                message.value,
            )
            .await?;

        Ok(TxResult {
            result: Program {
                id,
                backend: self.clone(),
            },
            logs: vec![],
        })
    }

    async fn send<M>(&self, id: ProgramId, message: M) -> Result<TxResult<MessageId>>
    where
        M: Into<Message> + Send,
    {
        let message = message.into();
        self.switch_pair(message.signer).await?;

        let api = self.api().await;
        let (mid, _hash) = api
            .send_message_bytes(
                id,
                message.payload,
                message.gas_limit.unwrap_or(api.block_gas_limit()?),
                message.value,
            )
            .await?;

        Ok(TxResult {
            result: mid,
            logs: vec![],
        })
    }

    async fn message(&self, mid: MessageId) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        self.inner
            .lock()
            .await
            .get_mailbox_message(mid)
            .await
            .map_err(Into::into)
    }
}

impl From<GearApi> for GClient {
    fn from(api: GearApi) -> Self {
        Self {
            inner: Arc::new(Mutex::new(api)),
            pairs: HashMap::from_iter(vec![(ALICE, "//Alice".to_string())].into_iter()),
        }
    }
}
