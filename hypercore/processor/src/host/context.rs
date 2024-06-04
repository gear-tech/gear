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

use gear_core::ids::{prelude::CodeIdExt, CodeId, ProgramId};
use wasmtime::{Memory, Table};

pub struct HostContext {
    pub code: Vec<u8>,
    pub(crate) memory: Option<Memory>,
    pub(crate) table: Option<Table>,
}

impl HostContext {
    pub fn new(code: Vec<u8>) -> Self {
        Self {
            code,
            memory: None,
            table: None,
        }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn id(&self) -> CodeId {
        CodeId::generate(self.code())
    }

    pub fn len(&self) -> usize {
        self.code().len()
    }

    pub fn memory(&self) -> Memory {
        self.memory.unwrap()
    }

    pub fn table(&self) -> Table {
        self.table.unwrap()
    }
}
