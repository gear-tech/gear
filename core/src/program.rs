// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use anyhow::Result;
use codec::{Decode, Encode};
use core::convert::TryFrom;
use core::{cmp, fmt};

use crate::memory::{PageBuf, PageNumber};

/// Program identifier.
///
/// 256-bit program identifier. In production environments, should be the result of a cryptohash function.
#[derive(
    Clone, Copy, Decode, Default, Encode, derive_more::From, Hash, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct ProgramId([u8; 32]);

impl fmt::Display for ProgramId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = cmp::min(self.0.len(), f.precision().unwrap_or(self.0.len()));
        if let Ok(hex) = crate::util::encode_hex(&self.0[..p]) {
            write!(f, "{}", hex)
        } else {
            Err(fmt::Error)
        }
    }
}

impl fmt::Debug for ProgramId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl From<u64> for ProgramId {
    fn from(v: u64) -> Self {
        let mut id = ProgramId([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
    }
}

impl ProgramId {
    /// Create new program id from bytes.
    ///
    /// Will panic if slice is not 32 bytes length.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("Slice is not 32 bytes length")
        };
        let mut id = ProgramId([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }

    /// Return reference to raw bytes of this program id.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    /// Return mutable reference to raw bytes of this program id.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }

    /// System origin
    pub fn system() -> Self {
        Self([0u8; 32])
    }
}

/// Program.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: Vec<u8>,
    /// Initial memory export size.
    static_pages: u32,
    /// Saved state of memory pages.
    persistent_pages: BTreeMap<PageNumber, Box<PageBuf>>,
    /// Message nonce
    message_nonce: u64,
}

impl Program {
    /// New program with specific `id`, `code` and `persistent_memory`.
    pub fn new(id: ProgramId, code: Vec<u8>, pages: BTreeMap<u32, Vec<u8>>) -> Result<Self> {
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

        let mut persistent_pages: BTreeMap<PageNumber, Box<PageBuf>> = BTreeMap::new();

        for (num, buf) in pages {
            persistent_pages.insert(
                num.into(),
                Box::new(
                    PageBuf::try_from(buf).map_err(|_| {
                        anyhow::anyhow!("Error loading program: invalid page buffer")
                    })?,
                ),
            );
        }

        Ok(Program {
            id,
            code,
            static_pages,
            persistent_pages,
            message_nonce: 0,
        })
    }

    /// Reference to code of this program.
    pub fn code(&self) -> &[u8] {
        &self.code[..]
    }

    /// Get the id of this program.
    pub fn id(&self) -> ProgramId {
        self.id
    }

    /// Get initial memory size for this program.
    pub fn static_pages(&self) -> u32 {
        self.static_pages
    }

    /// Set the code of this program.
    pub fn set_code(&mut self, code: Vec<u8>) -> Result<()> {
        self.static_pages = {
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
        self.code = code;

        Ok(())
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

    /// Set memory page from buffer.
    pub fn set_page(&mut self, page: PageNumber, buf: &[u8]) -> Result<()> {
        self.persistent_pages.insert(
            page,
            Box::new(
                PageBuf::try_from(buf)
                    .map_err(|err| anyhow::format_err!("TryFromSlice err: {}", err))?,
            ),
        );
        Ok(())
    }

    /// Get reference to memory pages.
    pub fn get_pages(&self) -> &BTreeMap<PageNumber, Box<PageBuf>> {
        &self.persistent_pages
    }

    /// Get reference to memory page.
    #[allow(clippy::borrowed_box)]
    pub fn get_page(&self, page: PageNumber) -> Option<&Box<PageBuf>> {
        self.persistent_pages.get(&page)
    }

    /// Get mut reference to memory page.
    pub fn get_page_mut(&mut self, page: PageNumber) -> Option<&mut Box<PageBuf>> {
        self.persistent_pages.get_mut(&page)
    }

    /// Clear static area of this program.
    pub fn clear_memory(&mut self) {
        self.persistent_pages.clear();
    }

    /// Message nonce.
    pub fn message_nonce(&self) -> u64 {
        self.message_nonce
    }

    /// Set message nonce.
    pub fn set_message_nonce(&mut self, val: u64) {
        self.message_nonce = val;
    }

    /// Fetch and increment message nonce
    pub fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.message_nonce;
        self.message_nonce += 1;
        nonce
    }

    /// Reset the program.
    pub fn reset(&mut self, code: Vec<u8>) -> Result<()> {
        self.set_code(code)?;
        self.clear_memory();
        self.message_nonce = 0;

        Ok(())
    }
}

#[cfg(test)]
/// This module contains tests of `fn encode_hex(bytes: &[u8]) -> String`
/// and ProgramId's `fn from_slice(s: &[u8]) -> Self` constructor
mod tests {
    use super::{Program, ProgramId};
    use crate::util::encode_hex;
    use alloc::collections::BTreeMap;
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
    /// Test that `encode_hex(...)` encodes correctly
    fn hex_encoding() {
        let bytes = "foobar".as_bytes();
        let result = encode_hex(&bytes).unwrap();

        assert_eq!(result, "666f6f626172");
    }

    #[test]
    #[should_panic(expected = "Slice is not 32 bytes length")]
    /// Test that ProgramId's `from_slice(...)` constructor causes panic
    /// when the argument has the wrong length
    fn program_id_from_slice_error_implementation() {
        let bytes = b"foobar";
        let _ = ProgramId::from_slice(bytes);
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
        let mut pages = BTreeMap::new();

        // invalid PageBuf
        pages.insert(1, vec![]);

        assert!(Program::new(ProgramId::from(1), binary.clone(), pages.clone()).is_err());

        pages.insert(1, vec![0; 65537]);

        assert!(Program::new(ProgramId::from(1), binary.clone(), pages).is_err());

        let mut program = Program::new(ProgramId::from(1), binary, BTreeMap::default()).unwrap();

        // 2 initial pages
        assert_eq!(program.static_pages(), 2);

        assert!(program.set_page(1.into(), &vec![0; 123]).is_err());

        assert!(program.set_page(1.into(), &vec![0; 65536]).is_ok());
        assert_eq!(program.get_pages().len(), 1);
    }
}
