// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Module for memory and allocations context.

use crate::{
    buffer::LimitedVec,
    gas::ChargeError,
    pages::{
        numerated::{
            interval::{Interval, NewWithLenError, TryFromRangeError},
            Numerated,
        },
        GearPage, WasmPage, WasmPagesAmount,
    },
};
use alloc::{collections::BTreeSet, format};
use byteorder::{ByteOrder, LittleEndian};
use core::{
    fmt,
    fmt::Debug,
    ops::{Deref, DerefMut},
};
use scale_info::{
    scale::{self, Decode, Encode, EncodeLike, Input, Output},
    TypeInfo,
};

/// Interval in wasm program memory.
#[derive(Clone, Copy, Eq, PartialEq, Encode, Decode)]
pub struct MemoryInterval {
    /// Interval offset in bytes.
    pub offset: u32,
    /// Interval size in bytes.
    pub size: u32,
}

impl MemoryInterval {
    /// Convert `MemoryInterval` to `[u8; 8]` bytes.
    /// `0..4` - `offset`
    /// `4..8` - `size`
    #[inline]
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        LittleEndian::write_u32(&mut bytes[0..4], self.offset);
        LittleEndian::write_u32(&mut bytes[4..8], self.size);
        bytes
    }

    /// Convert `[u8; 8]` bytes to `MemoryInterval`.
    /// `0..4` - `offset`
    /// `4..8` - `size`
    #[inline]
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 8 {
            return Err("bytes size != 8");
        }
        let offset = LittleEndian::read_u32(&bytes[0..4]);
        let size = LittleEndian::read_u32(&bytes[4..8]);
        Ok(MemoryInterval { offset, size })
    }
}

impl From<(u32, u32)> for MemoryInterval {
    fn from(val: (u32, u32)) -> Self {
        MemoryInterval {
            offset: val.0,
            size: val.1,
        }
    }
}

impl From<MemoryInterval> for (u32, u32) {
    fn from(val: MemoryInterval) -> Self {
        (val.offset, val.size)
    }
}

impl Debug for MemoryInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "[offset: {:#x}, size: {:#x}]",
            self.offset, self.size
        ))
    }
}

/// Alias for inner type of page buffer.
pub type PageBufInner = LimitedVec<u8, (), { GearPage::SIZE as usize }>;

/// Buffer for gear page data.
#[derive(Clone, PartialEq, Eq, TypeInfo)]
pub struct PageBuf(PageBufInner);

// These traits are implemented intentionally by hand to achieve two goals:
// - store PageBuf as fixed size array in a storage to eliminate extra bytes
//      for length;
// - work with PageBuf as with Vec. This is to workaround a limit in 2_048
//      items for fixed length array in polkadot.js/metadata.
//      Grep 'Only support for [[]Type' to get more details on that.
impl Encode for PageBuf {
    fn size_hint(&self) -> usize {
        GearPage::SIZE as usize
    }

    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        dest.write(self.0.inner())
    }
}

impl Decode for PageBuf {
    #[inline]
    fn decode<I: Input>(input: &mut I) -> Result<Self, scale::Error> {
        let mut buffer = PageBufInner::new_default();
        input.read(buffer.inner_mut())?;
        Ok(Self(buffer))
    }
}

impl EncodeLike for PageBuf {}

impl Debug for PageBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PageBuf({:?}..{:?})",
            &self.0.inner()[0..10],
            &self.0.inner()[GearPage::SIZE as usize - 10..GearPage::SIZE as usize]
        )
    }
}

impl Deref for PageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.0.inner()
    }
}

impl DerefMut for PageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.inner_mut()
    }
}

impl PageBuf {
    /// Returns new page buffer with zeroed data.
    pub fn new_zeroed() -> PageBuf {
        Self(PageBufInner::new_default())
    }

    /// Creates PageBuf from inner buffer. If the buffer has
    /// the size of [`GearPage`] then no reallocations occur.
    /// In other case it will be extended with zeros.
    ///
    /// The method is implemented intentionally instead of trait From to
    /// highlight conversion cases in the source code.
    pub fn from_inner(mut inner: PageBufInner) -> Self {
        inner.extend_with(0);
        Self(inner)
    }
}

/// Host pointer type.
/// Host pointer can be 64bit or less, to support both we use u64.
pub type HostPointer = u64;

const _: () = assert!(core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>());

/// Core memory error.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
pub enum MemoryError {
    /// The error occurs in attempt to access memory outside wasm program memory.
    #[display(fmt = "Trying to access memory outside wasm program memory")]
    AccessOutOfBounds,
}

/// Backend wasm memory interface.
pub trait Memory {
    /// Memory grow error.
    type GrowError: Debug;

    /// Grow memory by number of pages.
    fn grow(&mut self, pages: WasmPagesAmount) -> Result<(), Self::GrowError>;

    /// Return current size of the memory.
    fn size(&self) -> WasmPagesAmount;

    /// Set memory region at specific pointer.
    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError>;

    /// Returns native addr of wasm memory buffer in wasm executor
    fn get_buffer_host_addr(&mut self) -> Option<HostPointer> {
        if self.size() == WasmPagesAmount::from(0) {
            None
        } else {
            // We call this method only in case memory size is not zero,
            // so memory buffer exists and has addr in host memory.
            unsafe { Some(self.get_buffer_host_addr_unsafe()) }
        }
    }

    /// Get buffer addr unsafe.
    /// # Safety
    /// if memory size is 0 then buffer addr can be garbage
    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer;
}

/// Pages allocations context for the running program.
#[derive(Debug)]
pub struct AllocationsContext {
    /// Pages which has been in storage before execution
    init_allocations: BTreeSet<WasmPage>,
    allocations: BTreeSet<WasmPage>,
    max_pages: WasmPagesAmount,
    static_pages: WasmPagesAmount,
}

/// Before and after memory grow actions.
#[must_use]
pub trait GrowHandler {
    /// Before grow action
    fn before_grow_action(mem: &mut impl Memory) -> Self;
    /// After grow action
    fn after_grow_action(self, mem: &mut impl Memory);
}

/// Grow handler do nothing implementation
pub struct NoopGrowHandler;

impl GrowHandler for NoopGrowHandler {
    fn before_grow_action(_mem: &mut impl Memory) -> Self {
        NoopGrowHandler
    }
    fn after_grow_action(self, _mem: &mut impl Memory) {}
}

/// Incorrect allocation data error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
#[display(fmt = "Allocated memory pages or memory size are incorrect")]
pub struct IncorrectAllocationDataError;

/// Allocation error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum AllocError {
    /// Incorrect allocation data error
    #[from]
    #[display(fmt = "{_0}")]
    IncorrectAllocationData(IncorrectAllocationDataError),
    /// The error occurs when a program tries to allocate more memory than
    /// allowed.
    #[display(fmt = "Trying to allocate more wasm program memory than allowed")]
    ProgramAllocOutOfBounds,
    /// The error occurs in attempt to free-up a memory page from static area or
    /// outside additionally allocated for this program.
    #[display(fmt = "{_0:?} cannot be freed by the current program")]
    InvalidFree(WasmPage),
    /// Invalid range for free_range
    #[display(fmt = "Invalid range {_0:?}..={_1:?} for free_range")]
    InvalidFreeRange(WasmPage, WasmPage),
    /// Gas charge error
    #[from]
    #[display(fmt = "{_0}")]
    GasCharge(ChargeError),
}

impl AllocationsContext {
    /// New allocations context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    pub fn new(
        allocations: BTreeSet<WasmPage>,
        static_pages: WasmPagesAmount,
        max_pages: WasmPagesAmount,
    ) -> Self {
        Self {
            init_allocations: allocations.clone(),
            allocations,
            max_pages,
            static_pages,
        }
    }

    /// Return `true` if the page is the initial page,
    /// it means that the page was already in the storage.
    pub fn is_init_page(&self, page: WasmPage) -> bool {
        self.init_allocations.contains(&page)
    }

    /// Allocates specified number of continuously going pages
    /// and returns zero-based number of the first one.
    pub fn alloc<G: GrowHandler>(
        &mut self,
        pages: WasmPagesAmount,
        mem: &mut impl Memory,
        charge_gas_for_grow: impl FnOnce(WasmPagesAmount) -> Result<(), ChargeError>,
    ) -> Result<WasmPage, AllocError> {
        // TODO: Temporary solution to avoid panics, should be removed in #3791.
        // Presently, this error cannot appear because we have limit 512 wasm pages.
        let (Some(end_mem_page), Some(end_static_page)) = (
            mem.size().to_page_number(),
            self.static_pages.to_page_number(),
        ) else {
            return Err(IncorrectAllocationDataError.into());
        };

        let mut start = end_static_page;
        for &end in self.allocations.iter() {
            match Interval::<WasmPage>::try_from(start..end) {
                Ok(interval) if interval.len() >= pages => break,
                Err(TryFromRangeError::IncorrectRange) => {
                    return Err(IncorrectAllocationDataError.into())
                }
                _ => {}
            };

            start = end
                .inc()
                .to_page_number()
                .ok_or(AllocError::ProgramAllocOutOfBounds)?;
        }

        let interval = match Interval::with_len(start, u32::from(pages)) {
            Ok(interval) => interval,
            Err(NewWithLenError::OutOfBounds) => return Err(AllocError::ProgramAllocOutOfBounds),
            Err(NewWithLenError::ZeroLen) => {
                // Returns end of static pages in case `pages` == 0,
                // in order to support `alloc` legacy behavior.
                return Ok(end_static_page);
            }
        };

        if interval.end() >= self.max_pages {
            return Err(AllocError::ProgramAllocOutOfBounds);
        }

        if let Ok(extra_grow) = Interval::<WasmPage>::try_from(end_mem_page..=interval.end()) {
            charge_gas_for_grow(extra_grow.len())?;
            let grow_handler = G::before_grow_action(mem);
            mem.grow(extra_grow.len())
                .unwrap_or_else(|err| unreachable!("Failed to grow memory: {:?}", err));
            grow_handler.after_grow_action(mem);
        }

        self.allocations.extend(interval.iter());

        Ok(start)
    }

    /// Free specific memory page.
    pub fn free(&mut self, page: WasmPage) -> Result<(), AllocError> {
        if page < self.static_pages || page >= self.max_pages {
            return Err(AllocError::InvalidFree(page));
        }

        if !self.allocations.remove(&page) {
            return Err(AllocError::InvalidFree(page));
        }

        Ok(())
    }

    /// Try to free pages in range. Will only return error if range is invalid.
    ///
    /// Currently running program should own this pages.
    pub fn free_range(&mut self, interval: Interval<WasmPage>) -> Result<(), AllocError> {
        let (start, end) = interval.into_parts();

        if start < self.static_pages || end >= self.max_pages {
            return Err(AllocError::InvalidFreeRange(start, end));
        }

        self.allocations.retain(|p| !p.enclosed_by(&start, &end));

        Ok(())
    }

    /// Decomposes this instance and returns allocations.
    pub fn into_parts(self) -> (WasmPagesAmount, BTreeSet<WasmPage>, BTreeSet<WasmPage>) {
        (self.static_pages, self.init_allocations, self.allocations)
    }
}

/// This module contains tests of `GearPage` and `AllocationContext`
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    struct TestMemory(WasmPagesAmount);

    impl Memory for TestMemory {
        type GrowError = ();

        fn grow(&mut self, pages: WasmPagesAmount) -> Result<(), Self::GrowError> {
            self.0 = self.0.add(pages).ok_or(())?;
            Ok(())
        }

        fn size(&self) -> WasmPagesAmount {
            self.0
        }

        fn write(&mut self, _offset: u32, _buffer: &[u8]) -> Result<(), MemoryError> {
            unimplemented!()
        }

        fn read(&self, _offset: u32, _buffer: &mut [u8]) -> Result<(), MemoryError> {
            unimplemented!()
        }

        unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
            unimplemented!()
        }
    }

    #[test]
    fn page_buf() {
        let _ = env_logger::try_init();

        let mut data = PageBufInner::filled_with(199u8);
        data.inner_mut()[1] = 2;
        let page_buf = PageBuf::from_inner(data);
        log::debug!("page buff = {:?}", page_buf);
    }

    #[test]
    fn free_fails() {
        let mut ctx = AllocationsContext::new(Default::default(), 0.into(), 0.into());
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1.into())));

        let mut ctx = AllocationsContext::new(Default::default(), 1.into(), 0.into());
        assert_eq!(ctx.free(0.into()), Err(AllocError::InvalidFree(0.into())));

        let mut ctx = AllocationsContext::new(
            [WasmPage::from(0)].into_iter().collect(),
            1.into(),
            1.into(),
        );
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1.into())));

        let mut ctx = AllocationsContext::new(
            [WasmPage::from(1), WasmPage::from(3)].into_iter().collect(),
            1.into(),
            4.into(),
        );
        let interval = Interval::<WasmPage>::try_from(1u16..4).unwrap();
        assert_eq!(ctx.free_range(interval), Ok(()));
    }

    #[test]
    fn alloc() {
        let _ = env_logger::try_init();

        let alloc_ok =
            |ctx: &mut AllocationContext, mem: &mut TestMemory, pages: u16, expected: u16| {
                let res = ctx.alloc::<NoopGrowHandler>(pages.into(), mem, |_| Ok(()));
                assert_eq!(res, Ok(expected.into()));
            };

        let alloc_err =
            |ctx: &mut AllocationContext, mem: &mut TestMemory, pages: u16, err: AllocError| {
                let res = ctx.alloc::<NoopGrowHandler>(pages.into(), mem, |_| Ok(()));
                assert_eq!(res, Err(err));
            };

        let mut ctx = AllocationsContext::new(Default::default(), 16.into(), 256.into());
        let mut mem = TestMemory(16.into());
        alloc_ok(&mut ctx, &mut mem, 16, 16);
        alloc_ok(&mut ctx, &mut mem, 0, 16);

        // there is a space for 14 more
        (2..16).for_each(|i| alloc_ok(&mut ctx, &mut mem, 16, i * 16));

        // no more mem!
        alloc_err(&mut ctx, &mut mem, 16, AllocError::ProgramAllocOutOfBounds);

        // but we free some and then can allocate page that was freed
        ctx.free(137.into()).unwrap();
        alloc_ok(&mut ctx, &mut mem, 1, 137);

        // if we free 2 in a row we can allocate even 2
        ctx.free(117.into()).unwrap();
        ctx.free(118.into()).unwrap();
        alloc_ok(&mut ctx, &mut mem, 2, 117);

        // same as above, if we free_range 2 in a row we can allocate 2
        ctx.free_range(117.into()..=118.into()).unwrap();
        alloc_ok(&mut ctx, &mut mem, 2, 117);

        // but if 2 are not in a row, bad luck
        ctx.free(117.into()).unwrap();
        ctx.free(158.into()).unwrap();
        alloc_err(&mut ctx, &mut mem, 2, AllocError::ProgramAllocOutOfBounds);

        // test incorrect allocation data now
        let allocations = [1.into()].into_iter().collect();

        let mut ctx = AllocationsContext::new(allocations.clone(), 10.into(), 13.into());
        let mut mem = TestMemory(0.into());
        alloc_err(&mut ctx, &mut mem, 1, IncorrectAllocationDataError.into());

        let mut ctx =
            AllocationsContext::new(allocations.clone(), WasmPagesAmount::UPPER, 13.into());
        let mut mem = TestMemory(0.into());
        alloc_err(&mut ctx, &mut mem, 1, IncorrectAllocationDataError.into());

        let mut ctx =
            AllocationsContext::new(allocations.clone(), 10.into(), WasmPagesAmount::UPPER);
        let mut mem = TestMemory(0.into());
        alloc_err(&mut ctx, &mut mem, 1, AllocError::ProgramAllocOutOfBounds);
    }

    mod property_tests {
        use super::*;
        use proptest::{
            arbitrary::any, collection::size_range, prop_oneof, proptest, strategy::Strategy,
            test_runner::Config as ProptestConfig,
        };

        #[derive(Debug, Clone)]
        enum Action {
            Alloc { pages: WasmPagesAmount },
            Free { page: WasmPage },
            FreeRange { page: WasmPage, size: u8 },
        }

        fn actions() -> impl Strategy<Value = Vec<Action>> {
            let action = prop_oneof![
                wasm_pages_amount().prop_map(|pages| Action::Alloc { pages }),
                wasm_page().prop_map(|page| Action::Free { page }),
                (wasm_page(), any::<u8>())
                    .prop_map(|(page, size)| Action::FreeRange { page, size }),
            ];
            proptest::collection::vec(action, 0..1024)
        }

        fn allocations() -> impl Strategy<Value = BTreeSet<WasmPage>> {
            proptest::collection::btree_set(wasm_page(), size_range(0..1024))
        }

        fn wasm_page() -> impl Strategy<Value = WasmPage> {
            any::<u16>().prop_map(WasmPage::from)
        }

        fn wasm_pages_amount() -> impl Strategy<Value = WasmPagesAmount> {
            (0..u16::MAX as u32 + 1).prop_map(|x| {
                if x == u16::MAX as u32 + 1 {
                    WasmPagesAmount::UPPER
                } else {
                    WasmPagesAmount::from(x as u16)
                }
            })
        }

        fn proptest_config() -> ProptestConfig {
            ProptestConfig {
                cases: 1024,
                ..Default::default()
            }
        }

        #[track_caller]
        fn assert_alloc_error(err: AllocError) {
            match err {
                AllocError::IncorrectAllocationData(_) | AllocError::ProgramAllocOutOfBounds => {}
                err => panic!("{err:?}"),
            }
        }

        #[track_caller]
        fn assert_free_error(err: AllocError) {
            match err {
                AllocError::InvalidFree(_) => {}
                AllocError::InvalidFreeRange(_, _) => {}
                err => panic!("{err:?}"),
            }
        }

        proptest! {
            #![proptest_config(proptest_config())]
            #[test]
            fn alloc(
                static_pages in wasm_pages_amount(),
                allocations in allocations(),
                max_pages in wasm_pages_amount(),
                mem_size in wasm_pages_amount(),
                actions in actions(),
            ) {
                let _ = env_logger::try_init();

                let mut ctx = AllocationsContext::new(allocations, static_pages, max_pages);
                let mut mem = TestMemory {
                    max_pages: WasmPage::from(u16::MAX),
                    size: mem_size,
                };

                for action in actions {
                    match action {
                        Action::Alloc { pages } => {
                            if let Err(err) = ctx.alloc::<NoopGrowHandler>(pages, &mut mem, |_| Ok(())) {
                                assert_alloc_error(err);
                            }
                        }
                        Action::Free { page } => {
                            if let Err(err) = ctx.free(page) {
                                assert_free_error(err);
                            }
                        }
                        Action::FreeRange { page, size } => {
                            if let Ok(interval) = Interval::<WasmPage>::with_len(page, size as u32) {
                                let _ = ctx.free_range(interval).map_err(assert_free_error);
                            }
                        }
                    }
                }
            }
        }
    }
}
