// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Database library for hypercore.

mod mem;
mod rocks;
mod state;

pub use mem::MemDb;
pub use rocks::RocksDatabase;
pub use state::{Message, State};

use gear_core::code::InstrumentedCode;
use gprimitives::{CodeId, H256};

pub trait Database {
    /// Clone ref to database instance.
    fn clone_boxed(&self) -> Box<dyn Database>;

    /// Read code section.
    fn read_code(&self, code_id: CodeId) -> Option<Vec<u8>>;

    /// Write code section.
    fn write_code(&self, code_id: CodeId, code: &[u8]);

    /// Read instrumented code.
    fn read_instrumented_code(&self, code_id: CodeId) -> Option<InstrumentedCode>;

    /// Write instrumented code.
    fn write_instrumented_code(&self, code_id: CodeId, code: &InstrumentedCode);

    /// Read program state.
    fn read_state(&self, hash: H256) -> Option<State>;

    /// Write program state.
    fn write_state(&self, state: &State);
}

/// Content-addressable storage database.
pub trait CASDatabase: Send {
    /// Clone ref to database instance.
    fn clone_boxed(&self) -> Box<dyn CASDatabase>;

    /// Read data by hash.
    fn read(&self, hash: &H256) -> Option<Vec<u8>>;

    /// Write data, returns data hash.
    fn write(&self, data: &[u8]) -> H256;
}
