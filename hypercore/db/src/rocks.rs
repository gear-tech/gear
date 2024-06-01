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

use crate::{Database, State};
use anyhow::Result;
use gear_core::code::InstrumentedCode;
use gprimitives::{CodeId, H256};
use std::path::PathBuf;

/// Database for storing states and codes in memory.
#[derive(Debug, Clone)]
pub struct RocksDatabase;

impl RocksDatabase {
    //! Open database at specified
    pub fn open(path: PathBuf) -> Result<Self> {
        Ok(Self)
    }
}

impl Database for crate::RocksDatabase {
    fn clone_boxed(&self) -> Box<dyn Database> {
        Box::new(self.clone())
    }

    fn read_code(&self, _code_id: CodeId) -> Option<Vec<u8>> {
        unimplemented!()
    }

    fn read_instrumented_code(&self, _code_id: CodeId) -> Option<InstrumentedCode> {
        unimplemented!()
    }

    fn write_instrumented_code(&self, _code_id: CodeId, _code: &InstrumentedCode) {
        unimplemented!()
    }

    fn write_code(&self, _code_id: CodeId, _code: &[u8]) {
        unimplemented!()
    }

    fn read_state(&self, _hash: H256) -> Option<State> {
        unimplemented!()
    }

    fn write_state(&self, state: &State) {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn is_cloneable() {
        let db =
            RocksDatabase::open(PathBuf::from("/tmp")).expect("Failed to open database from /tmp");

        let _ = db.clone();
    }
}
