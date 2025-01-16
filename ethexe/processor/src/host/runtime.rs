// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use parity_wasm::elements::{Internal, Module};
use std::borrow::Cow;

pub enum Runtime {
    Raw(Cow<'static, [u8]>),
    Modified(Module),
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Module> for Runtime {
    fn from(module: Module) -> Self {
        Self::Modified(module)
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::Raw(ethexe_runtime::WASM_BINARY_BLOATY.unwrap().into())
    }

    pub fn from_code(code: Vec<u8>) -> Self {
        Self::Raw(code.into())
    }

    fn module_mut(&mut self) -> &mut Module {
        if let Self::Raw(bytes) = self {
            *self = Self::Modified(Module::from_bytes(bytes).unwrap());
        }

        let Self::Modified(module) = self else {
            unreachable!()
        };

        module
    }

    pub fn add_start_section(&mut self) {
        let module = self.module_mut();

        let start_fn_idx = module
            .export_section()
            .and_then(|section| {
                section.entries().iter().find_map(|export| {
                    if export.field() == "_start" {
                        if let Internal::Function(idx) = export.internal() {
                            Some(*idx)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            })
            .unwrap();

        module.set_start_section(start_fn_idx);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Raw(bytes) => bytes.to_vec(),
            Self::Modified(module) => module.into_bytes().unwrap(),
        }
    }
}
