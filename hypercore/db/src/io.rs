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
use gprimitives::H256;

pub trait Database {
    fn read_state(&self, hash: H256) -> Option<State>;
    fn write_state(&self, state: &State);
    fn read_code(&self, code_hash: H256) -> Option<Code>;
    fn write_code(&self, code: &Code);
    fn clone_boxed(&self) -> Box<dyn Database>;
}

impl Database for crate::RocksDatabase {
    fn read_state(&self, hash: H256) -> Option<State> {
        self.read_state(hash)
    }

    fn write_state(&self, state: &State) {
        self.write_state(state);
    }

    fn read_code(&self, code_hash: H256) -> Option<Code> {
        self.read_code(code_hash)
    }

    fn write_code(&self, code: &Code) {
        self.write_code(code);
    }

    fn clone_boxed(&self) -> Box<dyn Database> {
        Box::new(self.clone())
    }
}
