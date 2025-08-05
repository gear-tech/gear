// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Module for instrumented code.

use crate::code::Code;
use alloc::vec::Vec;
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

/// Instantiated section sizes for charging during module instantiation.
/// By "instantiated sections sizes" we mean the size of the section representation in the executor
/// during module instantiation.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct InstantiatedSectionSizes {
    /// Code section size in bytes.
    code_section: u32,
    /// Data section size in bytes based on the number of heuristic memory pages
    /// used during data section instantiation (see `GENERIC_OS_PAGE_SIZE`).
    data_section: u32,
    /// Global section size in bytes.
    global_section: u32,
    /// Table section size in bytes.
    table_section: u32,
    /// Element section size in bytes.
    element_section: u32,
    /// Type section size in bytes.
    type_section: u32,
}

impl InstantiatedSectionSizes {
    /// Creates a new instance of the section sizes.
    pub fn new(
        code_section: u32,
        data_section: u32,
        global_section: u32,
        table_section: u32,
        element_section: u32,
        type_section: u32,
    ) -> Self {
        Self {
            code_section,
            data_section,
            global_section,
            table_section,
            element_section,
            type_section,
        }
    }

    /// Returns the code section size in bytes.
    pub fn code_section(&self) -> u32 {
        self.code_section
    }

    /// Returns the data section size in bytes.
    pub fn data_section(&self) -> u32 {
        self.data_section
    }

    /// Returns the global section size in bytes.
    pub fn global_section(&self) -> u32 {
        self.global_section
    }

    /// Returns the table section size in bytes.
    pub fn table_section(&self) -> u32 {
        self.table_section
    }

    /// Returns the element section size in bytes.
    pub fn element_section(&self) -> u32 {
        self.element_section
    }

    /// Returns the type section size in bytes.
    pub fn type_section(&self) -> u32 {
        self.type_section
    }
}

/// The newtype contains the instrumented code and the corresponding id (hash).
#[derive(Clone, Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Hash)]
pub struct InstrumentedCode {
    /// Code instrumented with the latest schedule.
    bytes: Vec<u8>,
    /// Instantiated section sizes used for charging during module instantiation.
    instantiated_section_sizes: InstantiatedSectionSizes,
}

impl InstrumentedCode {
    /// Creates a new instance of the instrumented code.
    pub fn new(bytes: Vec<u8>, instantiated_section_sizes: InstantiatedSectionSizes) -> Self {
        Self {
            bytes,
            instantiated_section_sizes,
        }
    }

    /// Returns reference to the instrumented binary code.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns instantiated section sizes used for charging during module instantiation.
    pub fn instantiated_section_sizes(&self) -> &InstantiatedSectionSizes {
        &self.instantiated_section_sizes
    }

    /// Consumes the instance and returns the instrumented code.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl From<Code> for InstrumentedCode {
    fn from(code: Code) -> Self {
        code.into_parts().1
    }
}
