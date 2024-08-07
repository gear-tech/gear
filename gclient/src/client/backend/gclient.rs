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
    client::{Backend, Code, Message, Program, TxResult, ALICE},
    Event, GearApi, GearEvent,
};
use anyhow::{anyhow, Result};
use gear_core::{
    ids::ProgramId,
    message::{UserMessage, UserStoredMessage},
};
use gprimitives::{ActorId, MessageId, H256};
use gsdk::{
    ext::sp_core::{sr25519, Pair},
    metadata::runtime_types::gear_common::storage::primitives::Interval,
    Events,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::{Mutex, MutexGuard};

const MESSAGES_DEPTH: usize = 16;
const DEFAULT_TIMEOUT: u64 = 3000;

/// GClient instance
#[derive(Clone)]
pub struct GClient {
    inner: Arc<Mutex<GearApi>>,
    pairs: HashMap<ActorId, String>,
    messages: Arc<Mutex<BTreeMap<H256, Vec<UserMessage>>>>,
    timeout: Duration,
}

impl GClient {
    /// Create new gclient instance
    pub async fn new(api: GearApi) -> Result<Self> {
        let messages = Arc::new(Mutex::new(BTreeMap::new()));
        Self::spawn(api.subscribe_blocks().await?, messages.clone());

        Ok(Self {
            inner: Arc::new(Mutex::new(api)),
            pairs: HashMap::from_iter(vec![(ALICE, "//Alice".to_string())].into_iter()),
            timeout: Duration::from_millis(DEFAULT_TIMEOUT),
            messages,
        })
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

    /// Get user sent messages by block hash
    async fn logs(&self, hash: H256) -> Result<Vec<UserMessage>> {
        let now = SystemTime::now();
        loop {
            if now.elapsed()? > self.timeout {
                return Ok(vec![]);
            }

            if let Some(messages) = self.messages.lock().await.remove(&hash) {
                return Ok(messages);
            }
        }
    }

    /// Spawn gear messages
    fn spawn(mut sub: Events, gmessages: Arc<Mutex<BTreeMap<H256, Vec<UserMessage>>>>) {
        tokio::spawn(async move {
            while let Ok(Some((hash, events))) = sub.next_with_hash().await {
                let messages = events
                    .into_iter()
                    .filter_map(|e| {
                        if let Event::Gear(GearEvent::UserMessageSent { message, .. }) = e {
                            Some(message.into())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if messages.is_empty() {
                    continue;
                }

                let mut map = gmessages.lock().await;
                while map.len() > MESSAGES_DEPTH {
                    map.pop_first();
                }

                map.insert(hash, messages);
            }
        });
    }
}

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
        let wasm = code.bytes()?;
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
        let (mid, hash) = api
            .send_message_bytes(
                id,
                message.payload,
                message.gas_limit.unwrap_or(api.block_gas_limit()?),
                message.value,
            )
            .await?;

        Ok(TxResult {
            result: mid,
            logs: self.logs(hash).await?,
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

    fn add_pair(&mut self, suri: impl AsRef<str>) -> Result<()> {
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

    fn timeout(&mut self, timeout: Duration) {
        self.timeout = timeout
    }
}
