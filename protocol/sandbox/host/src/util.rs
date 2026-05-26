// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Utilities used by all backends

use crate::error::Result;
use sp_wasm_interface_common::Pointer;

/// Provides safe memory access interface using an external buffer
pub trait MemoryTransfer {
    /// Read data from a slice of memory into a newly allocated buffer.
    ///
    /// Returns an error if the read would go out of the memory bounds.
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> Result<Vec<u8>>;

    /// Read data from a slice of memory into a destination buffer.
    ///
    /// Returns an error if the read would go out of the memory bounds.
    fn read_into(&self, source_addr: Pointer<u8>, destination: &mut [u8]) -> Result<()>;

    /// Write data to a slice of memory.
    ///
    /// Returns an error if the write would go out of the memory bounds.
    fn write_from(&self, dest_addr: Pointer<u8>, source: &[u8]) -> Result<()>;

    /// Grow memory by `pages`.
    ///
    /// Returns memory prev size.
    fn memory_grow(&mut self, pages: u32) -> Result<u32>;

    /// Returns memory size in pages.
    fn memory_size(&mut self) -> u32;

    /// Returns host pointer to the wasm memory buffer.
    fn get_buff(&mut self) -> *mut u8;
}
