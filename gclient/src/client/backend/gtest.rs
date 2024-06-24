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

use crate::client::{Backend, Code, Message};
use anyhow::Result;
use async_trait::async_trait;
use gear_core::ids::ProgramId;
use gtest::{Program, System};
use std::{collections::HashMap, sync::Arc};

/// gear general client gtest backend
pub struct Gtest {
    /// gtest system
    system: Arc<System>,
}

#[async_trait]
impl Backend for Gtest {
    async fn deploy(&self, _code: impl Code) -> Result<()> {
        Ok(())
    }

    async fn send<M>(&self, _id: ProgramId, message: M) -> Result<()>
    where
        M: Into<Message> + Send,
    {
        Ok(())
    }
}
