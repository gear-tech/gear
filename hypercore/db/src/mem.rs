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

#[derive(Debug, Clone)]
pub struct MemDb {
    states: HashMap<Hash, State>,
    codes: HashMap<Hash, Code>,
}

impl MemDb {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            codes: HashMap::new(),
        }
    }
}

impl Database for MemDb {
    fn read_state(&self, hash: Hash) -> Option<State> {
        self.states.get(&hash).cloned()
    }

    fn write_state(&mut self, state: &State) {
        self.states.insert(state.hash(), state.clone());
    }

    fn read_code(&self, code_hash: Hash) -> Option<Code> {
        self.codes.get(&code_hash).cloned()
    }

    fn write_code(&mut self, code: &Code) {
        self.codes.insert(code.hash(), code.clone());
    }

    fn clone_boxed(&self) -> Box<dyn Database> {
        Box::new(self.clone())
    }
}
