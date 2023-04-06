// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! wasmi extensions for memory.

use crate::state::HostState;
use gear_core::{
    env::Ext,
    memory::{HostPointer, Memory, PageU32Size, WasmPage},
};
use gear_core_errors::MemoryError;
use wasmi::{core::memory_units::Pages, Memory as WasmiMemory, Store, StoreContextMut};

pub(crate) struct MemoryWrapRef<'a, E: Ext + 'static> {
    pub memory: WasmiMemory,
    pub store: StoreContextMut<'a, HostState<E>>,
}

impl<'a, E: Ext + 'static> Memory for MemoryWrapRef<'a, E> {
    type GrowError = wasmi::errors::MemoryError;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory
            .grow(&mut self.store, Pages(pages.raw() as usize))
            .map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.current_pages(&self.store).0 as u32)
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.data_mut(&mut self.store).as_mut().as_mut_ptr() as HostPointer
    }
}

/// Wrapper for [`wasmi::Memory`].
pub struct MemoryWrap<E: Ext + 'static> {
    pub(crate) memory: WasmiMemory,
    pub(crate) store: Store<HostState<E>>,
}

impl<E: Ext + 'static> MemoryWrap<E> {
    /// Wrap [`wasmi::Memory`] for Memory trait.
    pub(crate) fn new(memory: WasmiMemory, store: Store<HostState<E>>) -> Self {
        MemoryWrap { memory, store }
    }
    pub(crate) fn into_store(self) -> Store<HostState<E>> {
        self.store
    }
}

/// Memory interface for the allocator.
impl<E: Ext + 'static> Memory for MemoryWrap<E> {
    type GrowError = wasmi::errors::MemoryError;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory
            .grow(&mut self.store, Pages(pages.raw() as usize))
            .map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.current_pages(&self.store).0 as u32)
            .expect("Unexpected backend behavior: wasm memory is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.data_mut(&mut self.store).as_mut().as_mut_ptr() as HostPointer
    }
}

#[cfg(test)]
mod tests {
    use crate::state::State;

    use super::*;
    use gear_backend_common::{assert_err, assert_ok, mock::MockExt, ActorTerminationReason};
    use gear_core::memory::{AllocError, AllocationsContext, NoopGrowHandler};
    use wasmi::{Engine, Store};

    fn new_test_memory(
        static_pages: u16,
        max_pages: u16,
    ) -> (AllocationsContext, MemoryWrap<MockExt>) {
        use wasmi::MemoryType;

        let memory_type = MemoryType::new(static_pages as u32, Some(max_pages as u32));

        let engine = Engine::default();
        let mut store = Store::new(
            &engine,
            Some(State {
                ext: MockExt::default(),
                fallible_syscall_error: None,
                termination_reason: ActorTerminationReason::Success.into(),
            }),
        );

        let memory = WasmiMemory::new(&mut store, memory_type).expect("Memory creation failed");
        let memory = MemoryWrap::new(memory, store);

        (
            AllocationsContext::new(Default::default(), static_pages.into(), max_pages.into()),
            memory,
        )
    }

    #[test]
    fn smoky() {
        let (mut ctx, mut mem_wrap) = new_test_memory(16, 256);

        assert_ok!(
            ctx.alloc::<NoopGrowHandler>(16.into(), &mut mem_wrap, |_| Ok(())),
            16.into()
        );

        assert_ok!(
            ctx.alloc::<NoopGrowHandler>(0.into(), &mut mem_wrap, |_| Ok(())),
            16.into()
        );

        // there is a space for 14 more
        for _ in 0..14 {
            assert_ok!(ctx.alloc::<NoopGrowHandler>(16.into(), &mut mem_wrap, |_| Ok(())));
        }

        // no more mem!
        assert_err!(
            ctx.alloc::<NoopGrowHandler>(1.into(), &mut mem_wrap, |_| Ok(())),
            AllocError::ProgramAllocOutOfBounds
        );

        // but we free some
        assert_ok!(ctx.free(137.into()));

        // and now can allocate page that was freed
        assert_ok!(
            ctx.alloc::<NoopGrowHandler>(1.into(), &mut mem_wrap, |_| Ok(())),
            137.into()
        );

        // if we have 2 in a row we can allocate even 2
        assert_ok!(ctx.free(117.into()));
        assert_ok!(ctx.free(118.into()));

        assert_ok!(
            ctx.alloc::<NoopGrowHandler>(2.into(), &mut mem_wrap, |_| Ok(())),
            117.into()
        );

        // but if 2 are not in a row, bad luck
        assert_ok!(ctx.free(117.into()));
        assert_ok!(ctx.free(158.into()));

        assert_err!(
            ctx.alloc::<NoopGrowHandler>(2.into(), &mut mem_wrap, |_| Ok(())),
            AllocError::ProgramAllocOutOfBounds
        );
    }
}
