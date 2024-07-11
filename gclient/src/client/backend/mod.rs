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
    client::{Message, Program},
    TxResult,
};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::{ids::ProgramId, message::UserStoredMessage};
use gprimitives::MessageId;
use gsdk::metadata::runtime_types::gear_common::storage::primitives::Interval;
use parity_scale_codec::Decode;
use std::{fs, path::PathBuf};

mod gclient;
// mod gtest;

/// Backend for the general client
#[async_trait]
pub trait Backend: Sized {
    /// Get program instance
    async fn program(&self, id: ProgramId) -> Result<Program<Self>>;

    /// Add program to the backend
    ///
    /// NOTE: This interface implements `create_program` at the moment
    /// to simplify the usages.
    async fn deploy<M>(&self, _code: impl Code, message: M) -> Result<TxResult<Program<Self>>>
    where
        M: Into<Message> + Send;

    /// Send message
    async fn send<M>(&self, _id: ProgramId, message: M) -> Result<TxResult<MessageId>>
    where
        M: Into<Message> + Send;

    /// Get mailbox message from message id
    async fn message(&self, mid: MessageId) -> Result<Option<(UserStoredMessage, Interval<u32>)>>;

    /// Read program state from payload
    async fn state<R: Decode>(&self, id: ProgramId, payload: Vec<u8>) -> Result<R>;

    /// Read program state as bytes from payload
    async fn state_bytes(&self, id: ProgramId, payload: Vec<u8>) -> Result<Vec<u8>>;
}

/// Generate gear program code, could be path or bytes.
pub trait Code: Sized + Send {
    /// Get wasm bytes
    fn wasm(self) -> Result<Vec<u8>>;
}

impl Code for PathBuf {
    fn wasm(self) -> Result<Vec<u8>> {
        fs::read(self).map_err(Into::into)
    }
}

impl Code for Vec<u8> {
    fn wasm(self) -> Result<Vec<u8>> {
        Ok(self)
    }
}
