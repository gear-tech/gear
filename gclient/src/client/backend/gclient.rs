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
    client::{Backend, Code, Message, Program},
    GearApi, TxResult, WSAddress,
};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::{ids::ProgramId, message::UserStoredMessage};
use gprimitives::MessageId;
use gsdk::metadata::runtime_types::gear_common::storage::primitives::Interval;
use parity_scale_codec::Decode;
use std::{ops::Deref, sync::Arc};

/// GClient instance
#[derive(Clone)]
pub struct GClient {
    inner: Arc<GearApi>,
}

impl GClient {
    /// New gclient instance
    pub async fn new(address: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(GearApi::init(address.as_ref().parse()?).await?),
        })
    }
}

impl Deref for GClient {
    type Target = Arc<GearApi>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[async_trait]
impl Backend for GClient {
    async fn program(&self, id: ProgramId) -> Result<Program<Self>> {
        let _is_active = self.program_at(id, None).await?;
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
        let (_, id, _) = self
            .upload_program_bytes(
                wasm,
                message.salt,
                message.payload,
                message.gas_limit,
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
        let (mid, _) = self
            .send_message_bytes(id, message.payload, message.gas_limit, message.value)
            .await?;

        Ok(TxResult {
            result: mid,
            logs: vec![],
        })
    }

    async fn state<R: Decode>(&self, id: ProgramId, payload: Vec<u8>) -> Result<R> {
        self.read_state(id, payload).await.map_err(Into::into)
    }

    async fn state_bytes(&self, id: ProgramId, payload: Vec<u8>) -> Result<Vec<u8>> {
        self.read_state_bytes(id, payload).await.map_err(Into::into)
    }

    async fn message(&self, mid: MessageId) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        self.get_mailbox_message(mid).await.map_err(Into::into)
    }
}
