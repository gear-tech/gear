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
#![cfg(feature = "client")]
#![allow(unused)]

mod client;
mod packet;
mod program;

pub use self::{packet::Message, program::Program};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::ids::ProgramId;
use std::{fs, path::PathBuf};

/// Backend for the general client
#[async_trait]
pub trait Backend {
    /// Add program to the backend
    async fn deploy(&self, _code: impl Code) -> Result<()>;

    /// Send message
    async fn send(&self, _id: ProgramId, message: impl Into<Message>) -> Result<()>;
}

/// Generate gear program code, could be path or bytes.
pub trait Code: Sized + Send {
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
