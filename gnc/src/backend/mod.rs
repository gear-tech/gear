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

use crate::{Message, Program, TxResult};
use anyhow::{anyhow, Result};
pub use gclient::GClient;
use gear_core::{ids::ProgramId, message::UserStoredMessage};
use gprimitives::{ActorId, MessageId, H256};
use gsdk::metadata::runtime_types::gear_common::storage::primitives::Interval;
pub use gtest::GTest;
use std::{fs, path::PathBuf, time::Duration};

mod gclient;
mod gtest;

/// Backend for the general client
pub trait Backend: Sized {
    /// Get program instance
    async fn program(&self, id: ProgramId) -> Result<Program<Self>>;

    /// Add program to the backend
    ///
    /// NOTE: This interface implements `create_program` at the moment
    /// to simplify the usages.
    async fn deploy<M>(&self, code: impl Code, message: M) -> Result<TxResult<Program<Self>>>
    where
        M: Into<Message> + Send;

    /// Send message
    async fn send<M>(&self, id: ProgramId, message: M) -> Result<TxResult<MessageId>>
    where
        M: Into<Message> + Send;

    /// Get mailbox message from message id
    ///
    /// NOTE: this only works in gclient client
    async fn message(&self, _: MessageId) -> Result<Option<(UserStoredMessage, Interval<u32>)>> {
        Err(anyhow!(
            "gtest backend currently doesn't support this method"
        ))
    }

    /// Transfer balance to account
    ///
    /// NOTE: this function currently mints balance from air in the gtest client.
    async fn transfer(&self, to: ActorId, value: u128) -> Result<TxResult<H256>>;

    /// Set timeout for the backend
    fn timeout(&mut self, timeout: Duration);

    /// Add sr25519 pair to backend
    ///
    /// NOTE: the suri of the pairs will be stored in memory, use this method
    /// at your own risk! If you have better ideas to optimize this method, PRs
    /// are welcome!
    fn add_pair(&mut self, _: impl AsRef<str>) -> Result<()> {
        Ok(())
    }
}

/// Generate gear program code, could be path or bytes.
pub trait Code: Sized + Send {
    /// Get wasm bytes
    fn bytes(&self) -> Result<Vec<u8>>;
}

impl Code for PathBuf {
    fn bytes(&self) -> Result<Vec<u8>> {
        fs::read(self).map_err(Into::into)
    }
}

impl Code for Vec<u8> {
    fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.clone())
    }
}

impl Code for &[u8] {
    fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.to_vec())
    }
}
