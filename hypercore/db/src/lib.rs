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

use gear_core::code::InstrumentedCode;
use gprimitives::{CodeId, H256};

mod mem;
mod rocks;
mod state;

pub use mem::MemDb;
pub use rocks::RocksDatabase;
pub use state::{Message, State};

pub trait Database {
    // General section.
    fn clone_boxed(&self) -> Box<dyn Database>;

    // Original code section.
    fn read_code(&self, code_id: CodeId) -> Option<Vec<u8>>;

    fn write_code(&self, code_id: CodeId, code: &[u8]);

    // Instrumented code section.
    fn read_instrumented_code(&self, code_id: CodeId) -> Option<InstrumentedCode>;

    fn write_instrumented_code(&self, code_id: CodeId, code: &InstrumentedCode);

    // State section.
    fn read_state(&self, hash: H256) -> Option<State>;

    fn write_state(&self, state: &State);
}
