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

use crate::{Code, State};
use anyhow::Result;
use gprimitives::H256;
use std::path::PathBuf;

/// Database for storing states and codes in memory.
#[derive(Debug, Clone)]
pub struct RocksDatabase;

impl RocksDatabase {
    //! Open database at specified
    pub fn open(path: PathBuf) -> Result<Self> {
        Ok(Self)
    }

    pub fn read_state(&self, hash: H256) -> Option<State> {
        unimplemented!()
    }

    pub fn write_state(&self, state: &State) {
        unimplemented!()
    }

    pub fn read_code(&self, code_hash: H256) -> Option<Code> {
        unimplemented!()
    }

    pub fn remove_code(&self, code_hash: H256) {
        unimplemented!()
    }

    pub fn write_code(&self, code: &Code) {
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
