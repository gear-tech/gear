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
//! Gear general client
#![allow(async_fn_in_trait)]
mod backend;
mod packet;
mod program;

pub use self::{
    backend::{Backend, Code, GClient, GTest},
    packet::Message,
    program::Program,
};
use anyhow::{anyhow, Result};
use gclient::{GearApi, WSAddress};
use gear_core::message::UserMessage;
use gprimitives::ActorId;

/// Alice Actor Id
pub const ALICE: ActorId = ActorId::new([
    212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44, 133, 88, 133,
    76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125,
]);

/// Gear general client
pub struct Client<T: Backend> {
    backend: T,
}

impl<T: Backend> Client<T> {
    /// Create new client
    pub fn new(backend: T) -> Client<T> {
        Self { backend }
    }

    /// Add pair to the client
    pub fn add_pair(&mut self, suri: impl AsRef<str>) -> Result<()> {
        self.backend.add_pair(suri)
    }

    /// Deploy program to backend
    pub async fn deploy<M>(&self, code: impl Code, message: M) -> Result<TxResult<Program<T>>>
    where
        M: Into<Message> + Send,
    {
        self.backend.deploy(code, message).await
    }
}

impl Client<GTest> {
    /// New general client with `GTest` as backend
    pub fn gtest() -> Client<GTest> {
        Client::<GTest>::new(GTest::default())
    }
}

impl Client<GClient> {
    /// New general client with `GearApi` as backend
    ///
    /// NOTE: only websocket address and file path are supported.
    pub async fn gclient(uri: impl AsRef<str>) -> Result<Client<GClient>> {
        let uri = uri.as_ref();
        let api = if uri.starts_with("ws") {
            let patts = uri.split(':').collect::<Vec<_>>();
            let address = if patts.len() == 1 {
                WSAddress::try_new(uri, None)?
            } else if patts.len() == 2 {
                WSAddress::try_new(
                    format!("{}:{}", patts[0], patts[1]),
                    patts[2].parse::<u16>()?,
                )?
            } else {
                return Err(anyhow!("Invalid websocket address {uri}"));
            };
            GearApi::init(address).await?
        } else {
            GearApi::dev_from_path(uri).await?
        };

        Ok(Client::<GClient>::new(GClient::new(api).await?))
    }
}

/// Transaction result
#[derive(Debug, Clone)]
pub struct TxResult<T> {
    /// Result of this transaction
    pub result: T,
    /// Logs emitted in this transaction
    pub logs: Vec<UserMessage>,
}

impl<T> TxResult<Result<T>> {
    /// Create error result
    pub fn error(e: impl Into<anyhow::Error>) -> Self {
        Self {
            result: Err(e.into()),
            logs: Default::default(),
        }
    }

    /// resolve inner result from result wrapper
    pub fn ensure(self) -> Result<TxResult<T>> {
        let result = self.result?;
        Ok(TxResult {
            result,
            logs: self.logs,
        })
    }
}
