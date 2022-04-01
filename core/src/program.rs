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

//! Module for programs.

use crate::{
    code::Code,
    ids::ProgramId,
    memory::{PageBuf, PageNumber, WasmPageNumber},
};
use alloc::{boxed::Box, collections::BTreeMap, collections::BTreeSet, vec::Vec};
use anyhow::Result;
use codec::{Decode, Encode};
use core::convert::TryFrom;

/// Program.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: Code,
    /// Saved state of memory pages.
    persistent_pages: BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    /// Program is initialized.
    is_initialized: bool,
}

impl Program {
    /// New program with specific `id`, `code` and `persistent_memory`.
    pub fn new(id: ProgramId, code: Code) -> Self {
        Program {
            id,
            code,
            persistent_pages: Default::default(),
            is_initialized: false,
        }
    }

    /// New program from stored data
    pub fn from_parts(
        id: ProgramId,
        code: Code,
        persistent_pages_numbers: BTreeSet<PageNumber>,
        is_initialized: bool,
    ) -> Self {
        Self {
            id,
            code,
            persistent_pages: persistent_pages_numbers
                .into_iter()
                .map(|k| (k, None))
                .collect(),
            is_initialized,
        }
    }

    /// Reference to [`Code`] of this program.
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Reference to raw binary code of this program.
    pub fn raw_code(&self) -> &[u8] {
        self.code.code()
    }

    /// Get the [`ProgramId`] of this program.
    pub fn id(&self) -> ProgramId {
        self.id
    }

    /// Get initial memory size for this program.
    pub fn static_pages(&self) -> WasmPageNumber {
        self.code.static_pages()
    }

    /// Get whether program is initialized
    ///
    /// By default the [`Program`] is not initialized. The initialized status
    /// is set from the node.
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Set program initialized
    pub fn set_initialized(&mut self) {
        self.is_initialized = true;
    }

    /// Set memory from buffer.
    pub fn set_memory(&mut self, buffer: &[u8]) -> Result<()> {
        self.persistent_pages.clear();
        let boxed_slice: Box<[u8]> = buffer.into();
        // TODO: also alloc remainder.
        for (num, buf) in boxed_slice.chunks_exact(PageNumber::size()).enumerate() {
            self.set_page((num as u32 + 1).into(), buf)?;
        }
        Ok(())
    }

    /// Setting multiple pages
    pub fn set_pages(&mut self, pages: BTreeMap<PageNumber, Vec<u8>>) -> Result<()> {
        for (page_num, page_data) in pages {
            self.set_page(page_num, &page_data)?;
        }
        Ok(())
    }

    /// Set memory page from buffer.
    pub fn set_page(&mut self, page: PageNumber, buf: &[u8]) -> Result<()> {
        self.persistent_pages.insert(
            page,
            Option::from(Box::new(
                PageBuf::try_from(buf)
                    .map_err(|err| anyhow::format_err!("TryFromSlice err: {}", err))?,
            )),
        );
        Ok(())
    }

    /// Remove memory page from buffer.
    pub fn remove_page(&mut self, page: PageNumber) {
        self.persistent_pages.remove(&page);
    }

    /// Get reference to memory pages.
    pub fn get_pages(&self) -> &BTreeMap<PageNumber, Option<Box<PageBuf>>> {
        &self.persistent_pages
    }

    /// Get mut reference to memory pages.
    pub fn get_pages_mut(&mut self) -> &mut BTreeMap<PageNumber, Option<Box<PageBuf>>> {
        &mut self.persistent_pages
    }

    /// Get reference to memory page.
    #[allow(clippy::borrowed_box)]
    pub fn get_page_data(&self, page: PageNumber) -> Option<&Box<PageBuf>> {
        let res = self.persistent_pages.get(&page);
        res.expect("Page must be in persistent_pages").as_ref()
    }

    /// Get mut reference to memory page.
    pub fn get_page_mut(&mut self, page: PageNumber) -> Option<&mut Box<PageBuf>> {
        let res = self.persistent_pages.get_mut(&page);
        res.expect("Page must be in persistent_pages; mut").as_mut()
    }

    /// Clear static area of this program.
    pub fn clear_memory(&mut self) {
        self.persistent_pages.clear();
    }
}

#[cfg(test)]
/// This module contains tests of `fn encode_hex(bytes: &[u8]) -> String`
/// and ProgramId's `fn from_slice(s: &[u8]) -> Self` constructor
mod tests {
    use super::Program;
    use crate::code::Code;
    use crate::ids::ProgramId;
    use crate::memory::PageNumber;
    use alloc::{vec, vec::Vec};

    fn parse_wat(source: &str) -> Vec<u8> {
        let module_bytes = wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec();
        module_bytes
    }

    #[test]
    #[should_panic(expected = "Identifier must be 32 length")]
    /// Test that ProgramId's `from_slice(...)` constructor causes panic
    /// when the argument has the wrong length
    fn program_id_from_slice_error_implementation() {
        let bytes = "foobar";
        let _: ProgramId = bytes.as_bytes().into();
    }

    #[test]
    /// Test that Program constructor fails when pages can't be converted into PageBuf.
    fn program_memory() {
        let wat = r#"
            (module
                (import "env" "gr_reply_to"  (func $gr_reply_to (param i32)))
                (import "env" "memory" (memory 2))
                (export "handle" (func $handle))
                (export "handle_reply" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $handle_reply
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $init)
            )"#;

        let binary: Vec<u8> = parse_wat(wat);

        let code = Code::try_new(
            binary,
            1,
            None,
            wasm_instrument::gas_metering::ConstantCostRules::default(),
        )
        .unwrap();
        let mut program = Program::new(ProgramId::from(1), code);

        // 2 static pages
        assert_eq!(program.static_pages(), 2.into());

        assert!(program.set_page(1.into(), &[0; 123]).is_err());

        assert!(program
            .set_page(1.into(), &vec![0; PageNumber::size()])
            .is_ok());
        assert_eq!(program.get_pages().len(), 1);
    }
}
