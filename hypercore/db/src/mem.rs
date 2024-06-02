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
use gear_core::code::InstrumentedCode;
use gprimitives::{CodeId, H256};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Debug)]
pub struct MemDb {
    data: Arc<RwLock<MemDbData>>,
}

impl Default for MemDb {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
struct MemDbData {
    states: HashMap<H256, State>,
    original_codes: HashMap<CodeId, Vec<u8>>,
    instrumented_codes: HashMap<CodeId, InstrumentedCode>,
}

impl MemDb {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(MemDbData {
                states: HashMap::new(),
                original_codes: HashMap::new(),
                instrumented_codes: HashMap::new(),
            })),
        }
    }

    pub fn ref_clone(&self) -> Self {
        Self {
            data: self.data.clone(),
        }
    }
}

impl Database for MemDb {
    fn clone_boxed(&self) -> Box<dyn Database> {
        Box::new(self.ref_clone())
    }

    fn read_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.data
            .read()
            .unwrap()
            .original_codes
            .get(&code_id)
            .cloned()
    }

    fn write_code(&self, code_id: CodeId, code: &[u8]) {
        self.data
            .write()
            .unwrap()
            .original_codes
            .insert(code_id, code.to_vec());
    }

    fn read_instrumented_code(&self, code_id: CodeId) -> Option<InstrumentedCode> {
        self.data
            .read()
            .unwrap()
            .instrumented_codes
            .get(&code_id)
            .cloned()
    }

    fn write_instrumented_code(&self, code_id: CodeId, code: &InstrumentedCode) {
        self.data
            .write()
            .unwrap()
            .instrumented_codes
            .insert(code_id, code.clone());
    }

    fn read_state(&self, hash: H256) -> Option<State> {
        self.data.read().unwrap().states.get(&hash).cloned()
    }

    fn write_state(&self, state: &State) {
        self.data
            .write()
            .unwrap()
            .states
            .insert(state.hash(), state.clone());
    }
}
