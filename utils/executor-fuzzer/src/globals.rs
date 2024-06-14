// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use anyhow::Result;
use arbitrary::{Arbitrary, Unstructured};

const _GLOBAL_NAMES: [&str; 11] = [
    "gear_fuzz_a",
    "gear_fuzz_b",
    "gear_fuzz_c",
    "gear_fuzz_d",
    "gear_fuzz_e",
    "gear_fuzz_f",
    "gear_fuzz_g",
    "gear_fuzz_h",
    "gear_fuzz_i",
    "gear_fuzz_j",
    "gear_fuzz_k",
];

pub trait InstanceAccessGlobal {
    fn set_global(&self, name: &str, value: i64) -> Result<()>;
    fn get_global(&self, name: &str) -> Result<i64>;
}

#[derive(Arbitrary)]
pub struct GlobalListEntry {
    pub name_idx: usize,
    pub value: i64,
    pub get: bool,
    pub set: bool,
}

pub struct _GlobalList {
    list: Vec<GlobalListEntry>,
}

impl _GlobalList {
    pub fn _try_new(u: &mut Unstructured<'_>, len: usize) -> arbitrary::Result<Self> {
        let mut list = Vec::new();
        for _ in 0..len {
            list.push(GlobalListEntry::arbitrary(u)?);
        }
        Ok(Self { list })
    }

    fn _mutate_global(&self, instance: &mut dyn InstanceAccessGlobal) -> Result<()> {
        for entry in self.list.iter() {
            if entry.get {
                instance.get_global(_GLOBAL_NAMES[entry.name_idx])?;
            }
            if entry.set {
                instance.set_global(_GLOBAL_NAMES[entry.name_idx], entry.value)?;
            }
        }
        Ok(())
    }
}
