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

use super::*;
use gear_wasm_instrument::parity_wasm::{
    builder::{self, ModuleBuilder},
    elements::Instruction,
};
use gsys::HashWithValue;
use std::{mem, slice};

pub struct ModuleBuilderWithData {
    pub module_builder: ModuleBuilder,
    pub offsets: Vec<u32>,
    pub last_offset: u32,
}

impl ModuleBuilderWithData {
    pub fn new(addresses: &[HashWithValue], module: Module, memory_pages: WasmPageCount) -> Self {
        let module_builder = builder::from_module(module);
        if memory_pages == 0.into() {
            return Self {
                module_builder,
                offsets: vec![],
                last_offset: 0,
            };
        };

        let (module_builder, offsets, last_offset) =
            Self::inject_addresses(addresses, module_builder);
        Self {
            module_builder,
            offsets,
            last_offset,
        }
    }

    fn inject_addresses(
        addresses: &[HashWithValue],
        module_builder: ModuleBuilder,
    ) -> (ModuleBuilder, Vec<u32>, u32) {
        let size = mem::size_of::<HashWithValue>();
        addresses.iter().fold(
            (module_builder, Vec::with_capacity(addresses.len()), 0u32),
            |(module_builder, mut offsets, last_offset), address| {
                offsets.push(last_offset);
                let slice = unsafe {
                    slice::from_raw_parts(address as *const HashWithValue as *const u8, size)
                };
                let len = slice.len();
                let module_builder = module_builder
                    .data()
                    .offset(Instruction::I32Const(last_offset as i32))
                    .value(slice.to_vec())
                    .build();

                (module_builder, offsets, last_offset + len as u32)
            },
        )
    }
}
