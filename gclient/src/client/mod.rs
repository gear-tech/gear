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
#![cfg(feature = "client")]

mod backend;
mod packet;
mod program;

pub use self::{
    backend::{Backend, Code, GClient, GTest},
    packet::Message,
    program::Program,
};
use crate::GearApi;
use anyhow::Result;
use gear_core::message::UserMessage;
use gprimitives::ActorId;
pub use gsdk::ext::sp_core::{sr25519, Pair};

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

    /// Create gtest client
    pub fn gtest() -> Client<GTest> {
        Client::<GTest> {
            backend: GTest::default(),
        }
    }

    /// Create gclient client
    pub async fn gclient() -> Result<Client<GClient>> {
        Ok(Client::<GClient> {
            backend: GClient::from(GearApi::dev().await?),
        })
    }
}

impl<T: Backend> Client<T> {
    /// Deploy program to backend
    pub async fn deploy<M>(&self, code: impl Code, message: M) -> Result<TxResult<Program<T>>>
    where
        M: Into<Message> + Send,
    {
        self.backend.deploy(code, message).await
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
