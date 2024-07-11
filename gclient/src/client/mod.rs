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

mod backend;
mod instance;
mod packet;
mod program;

pub use self::{
    backend::{Backend, Code},
    instance::Client,
    packet::Message,
    program::Program,
};
use gear_core::message::UserMessage;

/// Transaction result
///
/// TODO: need a refactor on gclient side
pub struct TxResult<T> {
    /// Result of this transaction
    pub result: T,
    /// Logs emitted in this transaction
    pub logs: Vec<UserMessage>,
}
