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

//! sp-sandbox extensions for memory.

use gear_backend_common::state::HostState;
use gear_core::{
    env::Externalities,
    memory::{HostPointer, Memory, MemoryError},
    pages::{PageNumber, PageU32Size, WasmPage},
};
use gear_sandbox::{
    default_executor::{Caller, Store},
    SandboxMemory,
};

pub type DefaultExecutorMemory = gear_sandbox::default_executor::Memory;

pub(crate) struct MemoryWrapRef<'a, 'b: 'a, Ext: Externalities + 'static> {
    pub memory: DefaultExecutorMemory,
    pub caller: &'a mut Caller<'b, HostState<Ext, DefaultExecutorMemory>>,
}

impl<Ext: Externalities + 'static> Memory for MemoryWrapRef<'_, '_, Ext> {
    type GrowError = gear_sandbox::Error;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory.grow(self.caller, pages.raw()).map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.size(self.caller))
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(self.caller, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(self.caller, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.get_buff(self.caller) as HostPointer
    }
}

/// Wrapper for [`DefaultExecutorMemory`].
pub struct MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    pub(crate) memory: DefaultExecutorMemory,
    pub(crate) store: Store<HostState<Ext, DefaultExecutorMemory>>,
}

impl<Ext> MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    /// Wrap [`DefaultExecutorMemory`] for Memory trait.
    pub fn new(
        memory: DefaultExecutorMemory,
        store: Store<HostState<Ext, DefaultExecutorMemory>>,
    ) -> Self {
        MemoryWrap { memory, store }
    }

    pub(crate) fn into_store(self) -> Store<HostState<Ext, DefaultExecutorMemory>> {
        self.store
    }
}

/// Memory interface for the allocator.
impl<Ext> Memory for MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    type GrowError = gear_sandbox::Error;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory.grow(&mut self.store, pages.raw()).map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.size(&self.store))
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.get_buff(&mut self.store) as HostPointer
    }
}

/// can't be tested outside the node runtime
#[cfg(test)]
mod tests {
    use super::*;
    use gear_backend_common::{
        assert_err, assert_ok, mock::MockExt, state::State, ActorTerminationReason,
    };
    use gear_core::memory::{AllocError, AllocationsContext, GrowHandler};
    use gear_sandbox::{AsContextExt, SandboxStore};

    struct NoopGrowHandler;

    impl GrowHandler for NoopGrowHandler {
        fn before_grow_action(_mem: &mut impl Memory) -> Self {
            Self
        }

        fn after_grow_action(self, _mem: &mut impl Memory) {}
    }

    fn new_test_memory(
        static_pages: u16,
        max_pages: u16,
    ) -> (AllocationsContext, MemoryWrap<MockExt>) {
        use gear_sandbox::SandboxMemory as WasmMemory;

        let mut store = Store::new(None);
        let memory: DefaultExecutorMemory =
            WasmMemory::new(&mut store, static_pages as u32, Some(max_pages as u32))
                .expect("Memory creation failed");
        *store.data_mut() = Some(State {
            ext: MockExt::default(),
            memory: memory.clone(),
            termination_reason: ActorTerminationReason::Success.into(),
        });

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
