// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Module for checked code.

use alloc::vec::Vec;
use anyhow::Result;
use codec::{Decode, Encode};

/// Contains raw binary code of a program and initial memory size from memory import.
///
/// This entity ensures the code has passed several checks.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct CheckedCode {
    code: Vec<u8>,
    static_pages: u32,
}

impl CheckedCode {
    /// Checks provided code and creates new instance if the code is correct.
    pub fn try_new(code: Vec<u8>) -> Result<Self> {
        // get initial memory size from memory import.
        let static_pages: u32 = {
            parity_wasm::elements::Module::from_bytes(&code)
                .map_err(|e| anyhow::anyhow!("Error loading program: {}", e))?
                .import_section()
                .ok_or_else(|| anyhow::anyhow!("Error loading program: can't find import section"))?
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    parity_wasm::elements::External::Memory(mem_ty) => {
                        Some(mem_ty.limits().initial())
                    }
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("Error loading program: can't find memory export"))?
        };

        Ok(Self { code, static_pages })
    }

    /// Returns reference to the raw binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns initial memory size from memory import.
    pub fn static_pages(&self) -> u32 {
        self.static_pages
    }
}
