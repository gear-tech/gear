// This file is part of Gear.
//
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

/// Wrapper around [`wasmtime::Memory`] that implements [`sp_allocator::Memory`].
pub(crate) struct MemoryWrapper<'a, C>(&'a wasmtime::Memory, &'a mut C);

impl<'a, C> From<(&'a wasmtime::Memory, &'a mut C)> for MemoryWrapper<'a, C> {
    fn from((memory, caller): (&'a wasmtime::Memory, &'a mut C)) -> Self {
        Self(memory, caller)
    }
}

impl<C: wasmtime::AsContextMut> sp_allocator::Memory for MemoryWrapper<'_, C> {
    fn with_access<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
        run(self.0.data(&self.1))
    }

    fn with_access_mut<R>(&mut self, run: impl FnOnce(&mut [u8]) -> R) -> R {
        run(self.0.data_mut(&mut self.1))
    }

    fn grow(&mut self, additional: u32) -> std::result::Result<(), ()> {
        self.0
            .grow(&mut self.1, additional as u64)
            .map_err(|error| {
                log::error!("Failed to grow memory by {} pages: {}", additional, error)
            })
            .map(drop)
    }

    fn pages(&self) -> u32 {
        self.0.size(&self.1) as u32
    }

    fn max_pages(&self) -> Option<u32> {
        self.0.ty(&self.1).maximum().map(|pages| pages as _)
    }
}
