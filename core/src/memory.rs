// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    gas::ChargeError,
    limited::LimitedVec,
    pages::{GearPage, WasmPage, WasmPagesAmount},
};
use alloc::format;
use byteorder::{ByteOrder, LittleEndian};
use core::{
    fmt,
    fmt::Debug,
    ops::{Deref, DerefMut},
};
use numerated::{
    interval::{Interval, TryFromRangeError},
    tree::IntervalsTree,
};
use scale_info::{
    TypeInfo,
    scale::{self, Decode, Encode, EncodeLike, Input, Output},
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
        f.debug_struct("MemoryInterval")
            .field("offset", &format_args!("{:#x}", self.offset))
            .field("size", &format_args!("{:#x}", self.size))
            .finish()
    }
}

/// Error in attempt to make wrong size page buffer.
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, TypeInfo, derive_more::Display)]
#[display("Trying to make wrong size page buffer, must be {:#x}", GearPage::SIZE)]
pub struct IntoPageBufError;

/// Alias for inner type of page buffer.
pub type PageBufInner = LimitedVec<u8, { GearPage::SIZE as usize }>;

/// Buffer for gear page data.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, TypeInfo)]
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
        dest.write(&self.0)
    }
}

impl Decode for PageBuf {
    #[inline]
    fn decode<I: Input>(input: &mut I) -> Result<Self, scale::Error> {
        let mut buffer = PageBufInner::repeat(0);
        input.read(&mut buffer)?;
        Ok(Self(buffer))
    }
}

impl EncodeLike for PageBuf {}

impl Debug for PageBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PageBuf({:?}..{:?})",
            &self.0[0..10],
            &self.0[GearPage::SIZE as usize - 10..GearPage::SIZE as usize]
        )
    }
}

impl Deref for PageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PageBuf {
    /// Returns new page buffer with zeroed data.
    pub fn new_zeroed() -> PageBuf {
        Self(PageBufInner::repeat(0))
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

/// Page dump for the program.
#[derive(Clone)]
pub struct PageDump {
    /// Offset in memory.
    pub page: GearPage,
    /// Memory dump data.
    pub data: PageBuf,
}

impl Default for PageDump {
    fn default() -> Self {
        PageDump {
            page: GearPage::default(),
            data: PageBuf::new_zeroed(),
        }
    }
}

/// Memory dump for a program; number of stored pages is bounded by `u32::MAX`.
pub type MemoryDump = LimitedVec<PageDump, { u32::MAX as usize }>;

/// Host pointer type.
/// Host pointer can be 64bit or less, to support both we use u64.
pub type HostPointer = u64;

const _: () = assert!(size_of::<HostPointer>() >= size_of::<usize>());

/// Core memory error.
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, derive_more::Display)]
pub enum MemoryError {
    /// The error occurs in attempt to access memory outside wasm program memory.
    #[display("Trying to access memory outside wasm program memory")]
    #[default]
    AccessOutOfBounds,
}

/// Backend wasm memory interface.
pub trait Memory<Context> {
    /// Memory grow error.
    type GrowError: Debug;

    /// Grow memory by number of pages.
    fn grow(&self, ctx: &mut Context, pages: WasmPagesAmount) -> Result<(), Self::GrowError>;

    /// Return current size of the memory.
    fn size(&self, ctx: &Context) -> WasmPagesAmount;

    /// Set memory region at specific pointer.
    fn write(&self, ctx: &mut Context, offset: u32, buffer: &[u8]) -> Result<(), MemoryError>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, ctx: &Context, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError>;

    /// Returns native addr of wasm memory buffer in wasm executor
    fn get_buffer_host_addr(&self, ctx: &Context) -> Option<HostPointer> {
        if self.size(ctx) == WasmPagesAmount::from(0) {
            None
        } else {
            // We call this method only in case memory size is not zero,
            // so memory buffer exists and has addr in host memory.
            unsafe { Some(self.get_buffer_host_addr_unsafe(ctx)) }
        }
    }

    /// Get buffer addr unsafe.
    ///
    /// # Safety
    /// If memory size is 0 then buffer addr can be garbage
    unsafe fn get_buffer_host_addr_unsafe(&self, ctx: &Context) -> HostPointer;
}

/// Pages allocations context for the running program.
#[derive(Debug)]
pub struct AllocationsContext {
    /// Pages which has been in storage before execution
    allocations: IntervalsTree<WasmPage>,
    /// Shows that `allocations` was modified at least once per execution
    allocations_changed: bool,
    heap: Option<Interval<WasmPage>>,
    static_pages: WasmPagesAmount,
}

/// Before and after memory grow actions.
#[must_use]
pub trait GrowHandler<Context> {
    /// Before grow action
    fn before_grow_action(ctx: &mut Context, mem: &mut impl Memory<Context>) -> Self;
    /// After grow action
    fn after_grow_action(self, ctx: &mut Context, mem: &mut impl Memory<Context>);
}

/// Grow handler do nothing implementation
pub struct NoopGrowHandler;

impl<Context> GrowHandler<Context> for NoopGrowHandler {
    fn before_grow_action(_ctx: &mut Context, _mem: &mut impl Memory<Context>) -> Self {
        NoopGrowHandler
    }
    fn after_grow_action(self, _ctx: &mut Context, _mem: &mut impl Memory<Context>) {}
}

/// Inconsistency in memory parameters provided for wasm execution.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum MemorySetupError {
    /// Memory size exceeds max pages
    #[display("Memory size {memory_size:?} must be less than or equal to {max_pages:?}")]
    MemorySizeExceedsMaxPages {
        /// Memory size
        memory_size: WasmPagesAmount,
        /// Max allowed memory size
        max_pages: WasmPagesAmount,
    },
    /// Insufficient memory size
    #[display("Memory size {memory_size:?} must be at least {static_pages:?}")]
    InsufficientMemorySize {
        /// Memory size
        memory_size: WasmPagesAmount,
        /// Static memory size
        static_pages: WasmPagesAmount,
    },
    /// Stack end is out of static memory
    #[display("Stack end {stack_end:?} is out of static memory 0..{static_pages:?}")]
    StackEndOutOfStaticMemory {
        /// Stack end
        stack_end: WasmPage,
        /// Static memory size
        static_pages: WasmPagesAmount,
    },
    /// Allocated page is out of allowed memory interval
    #[display(
        "Allocated page {page:?} is out of allowed memory interval {static_pages:?}..{memory_size:?}"
    )]
    AllocatedPageOutOfAllowedInterval {
        /// Allocated page
        page: WasmPage,
        /// Static memory size
        static_pages: WasmPagesAmount,
        /// Memory size
        memory_size: WasmPagesAmount,
    },
}

/// Allocation error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum AllocError {
    /// The error occurs when a program tries to allocate more memory than
    /// allowed.
    #[display("Trying to allocate more wasm program memory than allowed")]
    ProgramAllocOutOfBounds,
    /// The error occurs in attempt to free-up a memory page from static area or
    /// outside additionally allocated for this program.
    #[display("{_0:?} cannot be freed by the current program")]
    InvalidFree(WasmPage),
    /// Invalid range for free_range
    #[display("Invalid range {_0:?}..={_1:?} for free_range")]
    InvalidFreeRange(WasmPage, WasmPage),
    /// Gas charge error
    GasCharge(ChargeError),
}

impl AllocationsContext {
    /// New allocations context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    ///
    /// Returns `MemorySetupError` on incorrect memory params.
    pub fn try_new(
        memory_size: WasmPagesAmount,
        allocations: IntervalsTree<WasmPage>,
        static_pages: WasmPagesAmount,
        stack_end: Option<WasmPage>,
        max_pages: WasmPagesAmount,
    ) -> Result<Self, MemorySetupError> {
        Self::validate_memory_params(
            memory_size,
            &allocations,
            static_pages,
            stack_end,
            max_pages,
        )?;

        let heap = match Interval::try_from(static_pages..max_pages) {
            Ok(interval) => Some(interval),
            Err(TryFromRangeError::EmptyRange) => None,
            // Branch is unreachable due to the check `static_pages <= max_pages`` in `validate_memory_params`.
            _ => unreachable!(),
        };

        Ok(Self {
            allocations,
            allocations_changed: false,
            heap,
            static_pages,
        })
    }

    /// Checks memory parameters, that are provided for wasm execution.
    /// NOTE: this params partially checked in `Code::try_new` in `gear-core`.
    fn validate_memory_params(
        memory_size: WasmPagesAmount,
        allocations: &IntervalsTree<WasmPage>,
        static_pages: WasmPagesAmount,
        stack_end: Option<WasmPage>,
        max_pages: WasmPagesAmount,
    ) -> Result<(), MemorySetupError> {
        if memory_size > max_pages {
            return Err(MemorySetupError::MemorySizeExceedsMaxPages {
                memory_size,
                max_pages,
            });
        }

        if static_pages > memory_size {
            return Err(MemorySetupError::InsufficientMemorySize {
                memory_size,
                static_pages,
            });
        }

        if let Some(stack_end) = stack_end
            && stack_end > static_pages
        {
            return Err(MemorySetupError::StackEndOutOfStaticMemory {
                stack_end,
                static_pages,
            });
        }

        if let Some(page) = allocations.end()
            && page >= memory_size
        {
            return Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page,
                static_pages,
                memory_size,
            });
        }
        if let Some(page) = allocations.start()
            && page < static_pages
        {
            return Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page,
                static_pages,
                memory_size,
            });
        }

        Ok(())
    }

    /// Allocates specified number of continuously going pages
    /// and returns zero-based number of the first one.
    pub fn alloc<Context, G: GrowHandler<Context>>(
        &mut self,
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        pages: WasmPagesAmount,
        charge_gas_for_grow: impl FnOnce(WasmPagesAmount) -> Result<(), ChargeError>,
    ) -> Result<WasmPage, AllocError> {
        // Empty heap means that all memory is static, then no pages can be allocated.
        // NOTE: returns an error even if `pages` == 0.
        let heap = self.heap.ok_or(AllocError::ProgramAllocOutOfBounds)?;

        // If trying to allocate zero pages, then returns heap start page (legacy).
        if pages == WasmPage::from(0) {
            return Ok(heap.start());
        }

        let interval = self
            .allocations
            .voids(heap)
            .find_map(|void| {
                Interval::<WasmPage>::with_len(void.start(), u32::from(pages))
                    .ok()
                    .and_then(|interval| (interval.end() <= void.end()).then_some(interval))
            })
            .ok_or(AllocError::ProgramAllocOutOfBounds)?;

        if let Ok(grow) = Interval::<WasmPage>::try_from(mem.size(ctx)..interval.end().inc()) {
            charge_gas_for_grow(grow.len())?;
            let grow_handler = G::before_grow_action(ctx, mem);
            mem.grow(ctx, grow.len()).unwrap_or_else(|err| {
                let err_msg = format!(
                    "AllocationContext:alloc: Failed to grow memory. \
                        Got error - {err:?}",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });
            grow_handler.after_grow_action(ctx, mem);
        }

        self.allocations.insert(interval);
        self.allocations_changed = true;

        Ok(interval.start())
    }

    /// Free specific memory page.
    pub fn free(&mut self, page: WasmPage) -> Result<(), AllocError> {
        if let Some(heap) = self.heap
            && page >= heap.start()
            && page <= heap.end()
            && self.allocations.remove(page)
        {
            self.allocations_changed = true;
            return Ok(());
        }

        Err(AllocError::InvalidFree(page))
    }

    /// Try to free pages in range. Will only return error if range is invalid.
    ///
    /// Currently running program should own this pages.
    pub fn free_range(&mut self, interval: Interval<WasmPage>) -> Result<(), AllocError> {
        if let Some(heap) = self.heap {
            // `free_range` allows do not modify the allocations so we do not check the `remove` result here
            if interval.start() >= heap.start() && interval.end() <= heap.end() {
                if self.allocations.remove(interval) {
                    self.allocations_changed = true;
                }

                return Ok(());
            }
        }

        Err(AllocError::InvalidFreeRange(
            interval.start(),
            interval.end(),
        ))
    }

    /// Decomposes this instance and returns `static_pages`, `allocations` and `allocations_changed` params.
    pub fn into_parts(self) -> (WasmPagesAmount, IntervalsTree<WasmPage>, bool) {
        (
            self.static_pages,
            self.allocations,
            self.allocations_changed,
        )
    }
}

/// This module contains tests of `GearPage` and `AllocationContext`
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::{cell::Cell, iter};

    struct TestMemory(Cell<WasmPagesAmount>);

    impl TestMemory {
        fn new(amount: WasmPagesAmount) -> Self {
            Self(Cell::new(amount))
        }
    }

    impl Memory<()> for TestMemory {
        type GrowError = ();

        fn grow(&self, _ctx: &mut (), pages: WasmPagesAmount) -> Result<(), Self::GrowError> {
            let new_pages_amount = self.0.get().add(pages).ok_or(())?;
            self.0.set(new_pages_amount);
            Ok(())
        }

        fn size(&self, _ctx: &()) -> WasmPagesAmount {
            self.0.get()
        }

        fn write(&self, _ctx: &mut (), _offset: u32, _buffer: &[u8]) -> Result<(), MemoryError> {
            unimplemented!()
        }

        fn read(&self, _ctx: &(), _offset: u32, _buffer: &mut [u8]) -> Result<(), MemoryError> {
            unimplemented!()
        }

        unsafe fn get_buffer_host_addr_unsafe(&self, _ctx: &()) -> HostPointer {
            unimplemented!()
        }
    }

    #[test]
    fn page_buf() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut data = PageBufInner::repeat(199u8);
        data[1] = 2;
        let page_buf = PageBuf::from_inner(data);
        log::debug!("page buff = {page_buf:?}");
    }

    #[test]
    fn free_fails() {
        let mut ctx =
            AllocationsContext::try_new(0.into(), Default::default(), 0.into(), None, 0.into())
                .unwrap();
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1.into())));

        let mut ctx = AllocationsContext::try_new(
            1.into(),
            [WasmPage::from(0)].into_iter().collect(),
            0.into(),
            None,
            1.into(),
        )
        .unwrap();
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1.into())));

        let mut ctx = AllocationsContext::try_new(
            4.into(),
            [WasmPage::from(1), WasmPage::from(3)].into_iter().collect(),
            1.into(),
            None,
            4.into(),
        )
        .unwrap();
        let interval = Interval::<WasmPage>::try_from(1u16..4).unwrap();
        assert_eq!(ctx.free_range(interval), Ok(()));
    }

    #[track_caller]
    fn alloc_ok(ctx: &mut AllocationsContext, mem: &mut TestMemory, pages: u16, expected: u16) {
        let res = ctx.alloc::<(), NoopGrowHandler>(&mut (), mem, pages.into(), |_| Ok(()));
        assert_eq!(res, Ok(expected.into()));
    }

    #[track_caller]
    fn alloc_err(ctx: &mut AllocationsContext, mem: &mut TestMemory, pages: u16, err: AllocError) {
        let res = ctx.alloc::<(), NoopGrowHandler>(&mut (), mem, pages.into(), |_| Ok(()));
        assert_eq!(res, Err(err));
    }

    #[test]
    fn alloc() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut ctx = AllocationsContext::try_new(
            256.into(),
            Default::default(),
            16.into(),
            None,
            256.into(),
        )
        .unwrap();
        let mut mem = TestMemory::new(16.into());
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
        let interval = Interval::<WasmPage>::try_from(117..119).unwrap();
        ctx.free_range(interval).unwrap();
        alloc_ok(&mut ctx, &mut mem, 2, 117);

        // but if 2 are not in a row, bad luck
        ctx.free(117.into()).unwrap();
        ctx.free(158.into()).unwrap();
        alloc_err(&mut ctx, &mut mem, 2, AllocError::ProgramAllocOutOfBounds);
    }

    #[test]
    fn memory_params_validation() {
        assert_eq!(
            AllocationsContext::validate_memory_params(
                4.into(),
                &iter::once(WasmPage::from(2)).collect(),
                2.into(),
                Some(2.into()),
                4.into(),
            ),
            Ok(())
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                4.into(),
                &Default::default(),
                2.into(),
                Some(2.into()),
                3.into(),
            ),
            Err(MemorySetupError::MemorySizeExceedsMaxPages {
                memory_size: 4.into(),
                max_pages: 3.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                1.into(),
                &Default::default(),
                2.into(),
                Some(1.into()),
                4.into(),
            ),
            Err(MemorySetupError::InsufficientMemorySize {
                memory_size: 1.into(),
                static_pages: 2.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                4.into(),
                &Default::default(),
                2.into(),
                Some(3.into()),
                4.into(),
            ),
            Err(MemorySetupError::StackEndOutOfStaticMemory {
                stack_end: 3.into(),
                static_pages: 2.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                4.into(),
                &[WasmPage::from(1), WasmPage::from(3)].into_iter().collect(),
                2.into(),
                Some(2.into()),
                4.into(),
            ),
            Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page: 1.into(),
                static_pages: 2.into(),
                memory_size: 4.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                4.into(),
                &[WasmPage::from(2), WasmPage::from(4)].into_iter().collect(),
                2.into(),
                Some(2.into()),
                4.into(),
            ),
            Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page: 4.into(),
                static_pages: 2.into(),
                memory_size: 4.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                13.into(),
                &iter::once(WasmPage::from(1)).collect(),
                10.into(),
                None,
                13.into()
            ),
            Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page: 1.into(),
                static_pages: 10.into(),
                memory_size: 13.into()
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                13.into(),
                &iter::once(WasmPage::from(1)).collect(),
                WasmPagesAmount::UPPER,
                None,
                13.into()
            ),
            Err(MemorySetupError::InsufficientMemorySize {
                memory_size: 13.into(),
                static_pages: WasmPagesAmount::UPPER
            })
        );

        assert_eq!(
            AllocationsContext::validate_memory_params(
                WasmPagesAmount::UPPER,
                &iter::once(WasmPage::from(1)).collect(),
                10.into(),
                None,
                WasmPagesAmount::UPPER,
            ),
            Err(MemorySetupError::AllocatedPageOutOfAllowedInterval {
                page: 1.into(),
                static_pages: 10.into(),
                memory_size: WasmPagesAmount::UPPER
            })
        );
    }

    #[test]
    fn allocations_changed_correctness() {
        let new_ctx = |allocations| {
            AllocationsContext::try_new(16.into(), allocations, 0.into(), None, 16.into()).unwrap()
        };

        // correct `alloc`
        let mut ctx = new_ctx(Default::default());
        assert!(
            !ctx.allocations_changed,
            "Expecting no changes after creation"
        );
        let mut mem = TestMemory::new(16.into());
        alloc_ok(&mut ctx, &mut mem, 16, 0);
        assert!(ctx.allocations_changed);

        let (_, allocations, allocations_changed) = ctx.into_parts();
        assert!(allocations_changed);

        // fail `alloc`
        let mut ctx = new_ctx(allocations);
        alloc_err(&mut ctx, &mut mem, 16, AllocError::ProgramAllocOutOfBounds);
        assert!(
            !ctx.allocations_changed,
            "Expecting allocations don't change because of error"
        );

        // fail `free`
        assert!(ctx.free(16.into()).is_err());
        assert!(!ctx.allocations_changed);

        // correct `free`
        assert!(ctx.free(10.into()).is_ok());
        assert!(ctx.allocations_changed);

        let (_, allocations, allocations_changed) = ctx.into_parts();
        assert!(allocations_changed);

        // correct `free_range`
        // allocations: [0..9] ∪ [11..15]
        let mut ctx = new_ctx(allocations);
        let interval = Interval::<WasmPage>::try_from(10u16..12).unwrap();
        assert!(ctx.free_range(interval).is_ok());
        assert!(
            ctx.allocations_changed,
            "Expected value is `true` because the 11th page was freed from allocations."
        );

        let (_, allocations, allocations_changed) = ctx.into_parts();
        assert!(allocations_changed);

        // fail `free_range`
        let mut ctx = new_ctx(allocations);
        let interval = Interval::<WasmPage>::try_from(0u16..17).unwrap();
        assert!(ctx.free_range(interval).is_err());
        assert!(!ctx.allocations_changed);
        assert!(!ctx.into_parts().2);
    }

    mod property_tests {
        use super::*;
        use proptest::{
            arbitrary::any,
            collection::size_range,
            prop_oneof, proptest,
            strategy::{Just, Strategy},
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
                // Allocate smaller number (0..32) of pages due to `BTree::extend` significantly slows down prop-test.
                wasm_pages_amount_with_range(0, 32).prop_map(|pages| Action::Alloc { pages }),
                wasm_page().prop_map(|page| Action::Free { page }),
                (wasm_page(), any::<u8>())
                    .prop_map(|(page, size)| Action::FreeRange { page, size }),
            ];
            proptest::collection::vec(action, 0..1024)
        }

        fn allocations(start: u16, end: u16) -> impl Strategy<Value = IntervalsTree<WasmPage>> {
            proptest::collection::btree_set(wasm_page_with_range(start, end), size_range(0..1024))
                .prop_map(|pages| pages.into_iter().collect::<IntervalsTree<WasmPage>>())
        }

        fn wasm_page_with_range(start: u16, end: u16) -> impl Strategy<Value = WasmPage> {
            (start..=end).prop_map(WasmPage::from)
        }

        fn wasm_page() -> impl Strategy<Value = WasmPage> {
            wasm_page_with_range(0, u16::MAX)
        }

        fn wasm_pages_amount_with_range(
            start: u32,
            end: u32,
        ) -> impl Strategy<Value = WasmPagesAmount> {
            (start..=end).prop_map(|x| {
                if x == u16::MAX as u32 + 1 {
                    WasmPagesAmount::UPPER
                } else {
                    WasmPagesAmount::from(x as u16)
                }
            })
        }

        fn wasm_pages_amount() -> impl Strategy<Value = WasmPagesAmount> {
            wasm_pages_amount_with_range(0, u16::MAX as u32 + 1)
        }

        #[derive(Debug)]
        struct MemoryParams {
            max_pages: WasmPagesAmount,
            mem_size: WasmPagesAmount,
            static_pages: WasmPagesAmount,
            allocations: IntervalsTree<WasmPage>,
        }

        // This high-order strategy generates valid memory parameters in a specific way that allows passing `AllocationContext::validate_memory_params` checks.
        fn combined_memory_params() -> impl Strategy<Value = MemoryParams> {
            wasm_pages_amount()
                .prop_flat_map(|max_pages| {
                    let mem_size = wasm_pages_amount_with_range(0, u32::from(max_pages));
                    (Just(max_pages), mem_size)
                })
                .prop_flat_map(|(max_pages, mem_size)| {
                    let static_pages = wasm_pages_amount_with_range(0, u32::from(mem_size));
                    (Just(max_pages), Just(mem_size), static_pages)
                })
                .prop_filter(
                    "filter out cases where allocation region has zero size",
                    |(_max_pages, mem_size, static_pages)| static_pages < mem_size,
                )
                .prop_flat_map(|(max_pages, mem_size, static_pages)| {
                    // Last allocated page should be < `mem_size`.
                    let end_exclusive = u32::from(mem_size) - 1;
                    (
                        Just(max_pages),
                        Just(mem_size),
                        Just(static_pages),
                        allocations(u32::from(static_pages) as u16, end_exclusive as u16),
                    )
                })
                .prop_map(
                    |(max_pages, mem_size, static_pages, allocations)| MemoryParams {
                        max_pages,
                        mem_size,
                        static_pages,
                        allocations,
                    },
                )
        }

        fn proptest_config() -> ProptestConfig {
            ProptestConfig {
                cases: 1024,
                ..Default::default()
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
                mem_params in combined_memory_params(),
                actions in actions(),
            ) {
                let _ = tracing_subscriber::fmt::try_init();

                let MemoryParams{max_pages, mem_size, static_pages, allocations} = mem_params;
                let mut ctx = AllocationsContext::try_new(mem_size, allocations, static_pages, None, max_pages).unwrap();

                let mut mem = TestMemory::new(mem_size);

                for action in actions {
                    match action {
                        Action::Alloc { pages } => {
                            match ctx.alloc::<_, NoopGrowHandler>(&mut (), &mut mem, pages, |_| Ok(())) {
                                Err(AllocError::ProgramAllocOutOfBounds) => {
                                    let x = mem.size(&()).add(pages);
                                    assert!(x.is_none() || x.unwrap() > max_pages);
                                }
                                Ok(page) => {
                                    assert!(pages == WasmPagesAmount::from(0) || (page >= static_pages && page < max_pages));
                                    assert!(mem.size(&()) <= max_pages);
                                    assert!(WasmPagesAmount::from(page).add(pages).unwrap() <= mem.size(&()));
                                }
                                Err(err) => panic!("{err:?}"),
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
