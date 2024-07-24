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
    TxResult,
};
use anyhow::Result;
use gear_core::ids::ProgramId;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Gear general client
pub struct Client<T: Backend> {
    backend: T,
    handle: Option<JoinHandle<()>>,
}

impl<T: Backend> Client<T> {
    /// Deploy program to backend
    pub async fn deploy<M>(&self, code: impl Code, message: M) -> Result<TxResult<Program<T>>>
    where
        M: Into<Message> + Send,
    {
        self.backend.deploy(code, message).await
    }

    // TODO: signer related methods
}
