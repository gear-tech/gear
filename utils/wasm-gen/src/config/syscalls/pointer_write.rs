// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::syscalls::PtrType;
use std::{collections::HashMap, mem::size_of, ops::RangeInclusive};

pub use gear_wasm_instrument::syscalls::ParamType;

#[derive(Debug, Clone)]
pub struct PointerWritesConfig(HashMap<PtrType, Vec<PointerWrite>>);

impl Default for PointerWritesConfig {
    fn default() -> PointerWritesConfig {
        let value_write_data = PointerWriteData::U128(0..=100_000_000_000);

        const HASH_LEN: usize = size_of::<gsys::Hash>() / size_of::<i32>();

        PointerWritesConfig(
            [
                (
                    PtrType::Value,
                    vec![PointerWrite {
                        offset: 0,
                        data: value_write_data.clone(),
                    }],
                ),
                (
                    PtrType::HashWithValue,
                    vec![PointerWrite {
                        offset: HASH_LEN,
                        data: value_write_data.clone(),
                    }],
                ),
                (
                    PtrType::TwoHashesWithValue,
                    vec![PointerWrite {
                        offset: 2 * HASH_LEN,
                        data: value_write_data,
                    }],
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}

impl PointerWritesConfig {
    pub fn empty() -> PointerWritesConfig {
        PointerWritesConfig(HashMap::new())
    }

    pub fn set_rule(&mut self, ptr_type: PtrType, pointer_writes: Vec<PointerWrite>) {
        self.0.insert(ptr_type, pointer_writes);
    }

    pub fn get_rule(&self, ptr_type: PtrType) -> Option<Vec<PointerWrite>> {
        self.0.get(&ptr_type).cloned()
    }
}

#[derive(Debug, Clone)]
pub struct PointerWrite {
    pub offset: usize,
    pub data: PointerWriteData,
}

#[derive(Debug, Clone)]
pub enum PointerWriteData {
    U128(RangeInclusive<u128>),
}

impl PointerWriteData {
    pub fn get_words(&self, unstructured: &mut Unstructured) -> Result<Vec<i32>> {
        match self {
            Self::U128(range) => {
                let value = unstructured.int_in_range(range.clone())?;
                Ok(value
                    .to_le_bytes()
                    .chunks(size_of::<u128>() / size_of::<i32>())
                    .map(|word_bytes| i32::from_le_bytes(word_bytes.try_into().unwrap()))
                    .collect())
            }
        }
    }
}
