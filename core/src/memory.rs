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

//! Module for memory and allocations context.

use crate::{buffer::LimitedVec, gas::ChargeError};
use alloc::{collections::BTreeSet, format};
use core::{
    fmt,
    fmt::Debug,
    iter,
    num::NonZeroU32,
    ops::{Deref, DerefMut},
};
use gear_core_errors::MemoryError;
use scale_info::{
    scale::{self, Decode, Encode, EncodeLike, Input, Output},
    TypeInfo,
};

/// A WebAssembly page has a constant size of 64KiB.
pub const WASM_PAGE_SIZE: usize = 0x10000;

/// A gear page size, currently 4KiB to fit the most common native page size.
/// This is size of memory data pages in storage.
/// So, in lazy-pages, when program tries to access some memory interval -
/// we can download just some number of gear pages instead of whole wasm page.
/// The number of small pages, which must be downloaded, is depends on host
/// native page size, so can vary.
pub const GEAR_PAGE_SIZE: usize = 0x4000;

static_assertions::const_assert!(WASM_PAGE_SIZE < u32::MAX as usize);
static_assertions::const_assert_eq!(WASM_PAGE_SIZE % GEAR_PAGE_SIZE, 0);

/// Interval in wasm program memory.
#[derive(Clone, Copy, Encode, Decode)]
pub struct MemoryInterval {
    /// Interval offset in bytes.
    pub offset: u32,
    /// Interval size in bytes.
    pub size: u32,
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
pub type PageBufInner = LimitedVec<u8, (), GEAR_PAGE_SIZE>;

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
        GEAR_PAGE_SIZE
    }

    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        dest.write(self.0.get())
    }
}

impl Decode for PageBuf {
    #[inline]
    fn decode<I: Input>(input: &mut I) -> Result<Self, scale::Error> {
        let mut buffer = PageBufInner::new_default();
        input.read(buffer.get_mut())?;
        Ok(Self(buffer))
    }
}

impl EncodeLike for PageBuf {}

impl Debug for PageBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "PageBuf({:?}..{:?})",
            &self.0.get()[0..10],
            &self.0.get()[GEAR_PAGE_SIZE - 10..GEAR_PAGE_SIZE]
        )
    }
}

impl Deref for PageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl DerefMut for PageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.get_mut()
    }
}

impl PageBuf {
    /// Returns new page buffer with zeroed data.
    pub fn new_zeroed() -> PageBuf {
        Self(PageBufInner::new_default())
    }

    /// Creates PageBuf from inner buffer. If the buffer has
    /// the size of GEAR_PAGE_SIZE then no reallocations occur. In other
    /// case it will be extended with zeros.
    ///
    /// The method is implemented intentionally instead of trait From to
    /// highlight conversion cases in the source code.
    pub fn from_inner(mut inner: PageBufInner) -> Self {
        inner.extend_with(0);
        Self(inner)
    }
}

/// Errors when act with PageU32Size.
#[derive(Debug, Clone, derive_more::Display)]
pub enum PageError {
    /// Addition overflow.
    #[display(fmt = "{_0} + {_1} overflows u32")]
    AddOverflowU32(u32, u32),
    /// Subtraction overflow.
    #[display(fmt = "{_0} - {_1} overflows u32")]
    SubOverflowU32(u32, u32),
    /// Overflow U32 memory size: has bytes, which has offset bigger then u32::MAX.
    #[display(fmt = "{_0} is too big to be number of page with size {_1}")]
    OverflowU32MemorySize(u32, u32),
    /// Cannot make pages iter from to.
    #[display(fmt = "Cannot make pages iter from {_0} to {_1}")]
    WrongRange(u32, u32),
}

/// U32 size pages iterator, to iterate continuously from one page to another.
#[derive(Debug, Clone)]
pub struct PagesIter<P: PageU32Size> {
    page: P,
    end: P,
}

impl<P: PageU32Size> Iterator for PagesIter<P> {
    type Item = P;

    fn next(&mut self) -> Option<Self::Item> {
        if self.page.raw() >= self.end.raw() {
            return None;
        };
        let res = self.page;
        unsafe {
            // Safe, because we checked that `page` is less than `end`.
            self.page = P::new_unchecked(self.page.raw() + 1);
        }
        Some(res)
    }
}

/// U32 size pages iterator, to iterate continuously from one page to another, including the last one.
#[derive(Debug, Clone)]
pub struct PagesIterInclusive<P: PageU32Size> {
    page: Option<P>,
    end: P,
}

impl<P: PageU32Size> Iterator for PagesIterInclusive<P> {
    type Item = P;

    fn next(&mut self) -> Option<Self::Item> {
        let page = self.page?;
        match self.end.raw() {
            end if end == page.raw() => self.page = None,
            end if end > page.raw() => unsafe {
                // Safe, because we checked that `page` is less than `end`.
                self.page = Some(P::new_unchecked(page.raw() + 1));
            },
            _ => unreachable!(
                "`page` {} cannot be bigger than `end` {}",
                page.raw(),
                self.end.raw(),
            ),
        }
        Some(page)
    }
}

impl<P: PageU32Size> PagesIterInclusive<P> {
    /// Returns current page.
    pub fn current(&self) -> Option<P> {
        self.page
    }
    /// Returns the end page.
    pub fn end(&self) -> P {
        self.end
    }
    /// Returns another page type iter, which pages intersect with `self` pages.
    pub fn convert<P1: PageU32Size>(&self) -> PagesIterInclusive<P1> {
        PagesIterInclusive::<P1> {
            page: self.page.map(|p| p.to_page()),
            end: self.end.to_last_page(),
        }
    }
}

/// Trait represents page with u32 size for u32 memory: max memory size is 2^32 bytes.
/// All operations with page guarantees, that no addr or page number can be overflowed.
pub trait PageU32Size: Sized + Clone + Copy + PartialEq + Eq + PartialOrd + Ord {
    /// Returns size of page.
    fn size_non_zero() -> NonZeroU32;
    /// Returns raw page number.
    fn raw(&self) -> u32;
    /// Constructs new page without any checks.
    /// # Safety
    /// Doesn't guarantee, that page offset or page end offset is in not overflowed.
    unsafe fn new_unchecked(num: u32) -> Self;

    /// Size as u32. Cannot be zero, because uses `Self::size_non_zero`.
    fn size() -> u32 {
        Self::size_non_zero().into()
    }
    /// Constructs new page from byte offset: returns page which contains this byte.
    fn from_offset(offset: u32) -> Self {
        unsafe { Self::new_unchecked(offset / Self::size()) }
    }
    /// Constructs new page from raw page number with checks.
    /// Returns error if page will contain bytes, with offsets bigger then u32::MAX.
    fn new(num: u32) -> Result<Self, PageError> {
        let page_begin = num
            .checked_mul(Self::size())
            .ok_or(PageError::OverflowU32MemorySize(num, Self::size()))?;
        let last_byte_offset = Self::size() - 1;
        // Check that the last page byte has index less or equal then u32::MAX
        page_begin
            .checked_add(last_byte_offset)
            .ok_or(PageError::OverflowU32MemorySize(num, Self::size()))?;
        // Now it is safe
        unsafe { Ok(Self::new_unchecked(num)) }
    }
    /// Returns page zero byte offset.
    fn offset(&self) -> u32 {
        self.raw() * Self::size()
    }
    /// Returns page last byte offset.
    fn end_offset(&self) -> u32 {
        self.raw() * Self::size() + (Self::size() - 1)
    }
    /// Returns new page, which contains `self` zero byte.
    fn to_page<P1: PageU32Size>(&self) -> P1 {
        P1::from_offset(self.offset())
    }
    /// Returns new page, which contains `self` last byte.
    fn to_last_page<P1: PageU32Size>(&self) -> P1 {
        P1::from_offset(self.end_offset())
    }
    /// Returns page which has number `page.raw() + raw`, with checks.
    fn add_raw(&self, raw: u32) -> Result<Self, PageError> {
        self.raw()
            .checked_add(raw)
            .map(Self::new)
            .ok_or(PageError::AddOverflowU32(self.raw(), raw))?
    }
    /// Returns page which has number `page.raw() - raw`, with checks.
    fn sub_raw(&self, raw: u32) -> Result<Self, PageError> {
        self.raw()
            .checked_sub(raw)
            .map(Self::new)
            .ok_or(PageError::SubOverflowU32(self.raw(), raw))?
    }
    /// Returns page which has number `page.raw() + other.raw()`, with checks.
    fn add(&self, other: Self) -> Result<Self, PageError> {
        self.add_raw(other.raw())
    }
    /// Returns page which has number `page.raw() - other.raw()`, with checks.
    fn sub(&self, other: Self) -> Result<Self, PageError> {
        self.sub_raw(other.raw())
    }
    /// Returns page which has number `page.raw() + 1`, with checks.
    fn inc(&self) -> Result<Self, PageError> {
        self.add_raw(1)
    }
    /// Returns page which has number `page.raw() - 1`, with checks.
    fn dec(&self) -> Result<Self, PageError> {
        self.sub_raw(1)
    }
    /// Aligns page zero byte and returns page which contains this byte.
    /// Normally if `size % Self::size() == 0`,
    /// then aligned byte is zero byte of the returned page.
    fn align_down(&self, size: NonZeroU32) -> Self {
        let size: u32 = size.into();
        Self::from_offset((self.offset() / size) * size)
    }
    /// Returns page, which has zero byte offset == 0.
    fn zero() -> Self {
        unsafe { Self::new_unchecked(0) }
    }
    /// Returns iterator `self`..`self` + `count`.
    fn iter_count(&self, count: Self) -> Result<PagesIter<Self>, PageError> {
        self.add(count).map(|end| PagesIter { page: *self, end })
    }
    /// Returns iterator `self`..`end`.
    fn iter_end(&self, end: Self) -> Result<PagesIter<Self>, PageError> {
        if end.raw() >= self.raw() {
            Ok(PagesIter { page: *self, end })
        } else {
            Err(PageError::WrongRange(self.raw(), end.raw()))
        }
    }
    /// Returns iterator `self`..=`end`.
    fn iter_end_inclusive(&self, end: Self) -> Result<PagesIterInclusive<Self>, PageError> {
        if end.raw() >= self.raw() {
            Ok(PagesIterInclusive {
                page: Some(*self),
                end,
            })
        } else {
            Err(PageError::WrongRange(self.raw(), end.raw()))
        }
    }
    /// Returns iterator `0`..=`self`
    fn iter_from_zero_inclusive(&self) -> PagesIterInclusive<Self> {
        PagesIterInclusive {
            page: Some(Self::zero()),
            end: *self,
        }
    }
    /// Returns iterator `0`..`self`
    fn iter_from_zero(&self) -> PagesIter<Self> {
        PagesIter {
            page: Self::zero(),
            end: *self,
        }
    }
    /// Returns iterator `self`..=`self`
    fn iter_once(&self) -> PagesIterInclusive<Self> {
        PagesIterInclusive {
            page: Some(*self),
            end: *self,
        }
    }
    /// To another page iterator. For example: PAGE1 has size 4 and PAGE2 has size 2:
    /// ````ignored
    /// Memory is splitted into PAGE1:
    /// [<====><====><====><====><====>]
    ///  0     1     2     3     4
    /// Memory splitted into PAGE2:
    /// [<=><=><=><=><=><=><=><=><=><=>]
    ///  0  1  2  3  4  5  6  7  8  9
    /// Then PAGE1 with number 2 contains [4, 5] pages of PAGE2,
    /// and we returns iterator over [4, 5] PAGE2.
    /// ````
    fn to_pages_iter<P: PageU32Size>(&self) -> PagesIterInclusive<P> {
        let start: P = self.to_page();
        let end: P = P::from_offset(self.end_offset());
        PagesIterInclusive {
            page: Some(start),
            end,
        }
    }
}

/// Page number.
#[derive(
    Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, Hash, TypeInfo, Default,
)]
pub struct GearPage(u32);

impl PageU32Size for GearPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(GEAR_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(GEAR_PAGE_SIZE as u32) }
    }

    fn raw(&self) -> u32 {
        self.0
    }

    unsafe fn new_unchecked(num: u32) -> Self {
        Self(num)
    }
}

/// Wasm page number.
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, TypeInfo, Default)]
pub struct WasmPage(u32);

impl PageU32Size for WasmPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(WASM_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(WASM_PAGE_SIZE as u32) }
    }

    fn raw(&self) -> u32 {
        self.0
    }

    unsafe fn new_unchecked(num: u32) -> Self {
        Self(num)
    }
}

impl From<u16> for WasmPage {
    fn from(value: u16) -> Self {
        // u16::MAX * WasmPage::size() - 1 == u32::MAX
        static_assertions::const_assert!(WASM_PAGE_SIZE == 0x10000);
        WasmPage(value as u32)
    }
}

impl From<u16> for GearPage {
    fn from(value: u16) -> Self {
        // u16::MAX * GearPage::size() - 1 <= u32::MAX
        static_assertions::const_assert!(GEAR_PAGE_SIZE <= 0x10000);
        GearPage(value as u32)
    }
}

/// Host pointer type.
/// Host pointer can be 64bit or less, to support both we use u64.
pub type HostPointer = u64;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

/// Backend wasm memory interface.
pub trait Memory {
    /// Memory grow error.
    type GrowError: Debug;

    /// Grow memory by number of pages.
    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError>;

    /// Return current size of the memory.
    fn size(&self) -> WasmPage;

    /// Set memory region at specific pointer.
    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError>;

    /// Returns native addr of wasm memory buffer in wasm executor
    fn get_buffer_host_addr(&mut self) -> Option<HostPointer> {
        if self.size() == 0.into() {
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
    max_pages: WasmPage,
    static_pages: WasmPage,
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
    #[display(fmt = "Page {_0} cannot be freed by the current program")]
    InvalidFree(u32),
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
        static_pages: WasmPage,
        max_pages: WasmPage,
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
        pages: WasmPage,
        mem: &mut impl Memory,
        charge_gas_for_grow: impl FnOnce(WasmPage) -> Result<(), ChargeError>,
    ) -> Result<WasmPage, AllocError> {
        let mem_size = mem.size();
        let mut start = self.static_pages;
        let mut start_page = None;
        for &end in self.allocations.iter().chain(iter::once(&mem_size)) {
            let page_gap = end.sub(start).map_err(|_| IncorrectAllocationDataError)?;

            if page_gap >= pages {
                start_page = Some(start);
                break;
            }

            start = end.inc().map_err(|_| AllocError::ProgramAllocOutOfBounds)?;
        }

        let start = if let Some(start) = start_page {
            start
        } else {
            // If we cannot find interval between already allocated pages, then try to alloc new pages.

            // Panic is impossible, because we check, that last allocated page can be incremented in loop above.
            let start = self
                .allocations
                .last()
                .map(|last| last.inc().unwrap_or_else(|err| {
                    unreachable!("Cannot increment last allocation: {}, but we checked in loop above that it can be done", err)
                }))
                .unwrap_or(self.static_pages);
            let end = start
                .add(pages)
                .map_err(|_| AllocError::ProgramAllocOutOfBounds)?;
            if end > self.max_pages {
                return Err(AllocError::ProgramAllocOutOfBounds);
            }

            // Panic is impossible, because in loop above we checked it.
            let extra_grow = end.sub(mem_size).unwrap_or_else(|err| {
                unreachable!(
                    "`mem_size` must be bigger than all allocations and static pages, but get {}",
                    err
                )
            });

            // Panic is impossible, in other case we would found interval inside existing memory.
            if extra_grow == WasmPage::zero() {
                unreachable!("`extra grow cannot be zero");
            }

            charge_gas_for_grow(extra_grow)?;

            let grow_handler = G::before_grow_action(mem);
            mem.grow(extra_grow)
                .unwrap_or_else(|err| unreachable!("Failed to grow memory: {:?}", err));
            grow_handler.after_grow_action(mem);

            start
        };

        // Panic is impossible, because we calculated `start` suitable for `pages`.
        let new_allocations = start
            .iter_count(pages)
            .unwrap_or_else(|err| unreachable!("`start` + `pages` is out of wasm memory: {}", err));

        self.allocations.extend(new_allocations);

        Ok(start)
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&mut self, page: WasmPage) -> Result<(), AllocError> {
        if page < self.static_pages || page >= self.max_pages || !self.allocations.remove(&page) {
            Err(AllocError::InvalidFree(page.0))
        } else {
            Ok(())
        }
    }

    /// Decomposes this instance and returns allocations.
    pub fn into_parts(self) -> (WasmPage, BTreeSet<WasmPage>, BTreeSet<WasmPage>) {
        (self.static_pages, self.init_allocations, self.allocations)
    }
}

#[cfg(test)]
/// This module contains tests of GearPage struct
mod tests {
    use super::*;

    use alloc::vec::Vec;

    #[test]
    /// Test that [GearPage] add up correctly
    fn page_number_addition() {
        let sum = GearPage(100).add(200.into()).unwrap();
        assert_eq!(sum, GearPage(300));
    }

    #[test]
    /// Test that [GearPage] subtract correctly
    fn page_number_subtraction() {
        let subtraction = GearPage(299).sub(199.into()).unwrap();
        assert_eq!(subtraction, GearPage(100))
    }

    #[test]
    /// Test that [WasmPage] set transforms correctly to [GearPage] set.
    fn wasm_pages_to_gear_pages() {
        let wasm_pages: Vec<WasmPage> = [0u32, 10u32].iter().copied().map(WasmPage).collect();
        let gear_pages: Vec<u32> = wasm_pages
            .iter()
            .flat_map(|p| p.to_pages_iter::<GearPage>())
            .map(|p| p.0)
            .collect();

        let expectation = [0, 1, 2, 3, 40, 41, 42, 43];

        assert!(gear_pages.eq(&expectation));
    }

    #[test]
    fn page_buf() {
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("gear_core=debug"),
        )
        .format_module_path(false)
        .format_level(true)
        .try_init()
        .expect("cannot init logger");

        let mut data = PageBufInner::filled_with(199u8);
        data.get_mut()[1] = 2;
        let page_buf = PageBuf::from_inner(data);
        log::debug!("page buff = {:?}", page_buf);
    }

    #[test]
    fn free_fails() {
        let mut ctx = AllocationsContext::new(BTreeSet::default(), WasmPage(0), WasmPage(0));
        assert_eq!(ctx.free(WasmPage(1)), Err(AllocError::InvalidFree(1)));

        let mut ctx = AllocationsContext::new(BTreeSet::default(), WasmPage(1), WasmPage(0));
        assert_eq!(ctx.free(WasmPage(0)), Err(AllocError::InvalidFree(0)));

        let mut ctx =
            AllocationsContext::new(BTreeSet::from([WasmPage(0)]), WasmPage(1), WasmPage(1));
        assert_eq!(ctx.free(WasmPage(1)), Err(AllocError::InvalidFree(1)));
    }

    #[test]
    fn page_iterator() {
        let test = |num1, num2| {
            let p1 = GearPage::from(num1);
            let p2 = GearPage::from(num2);

            assert_eq!(
                p1.iter_end(p2).unwrap().collect::<Vec<GearPage>>(),
                (num1..num2).map(GearPage::from).collect::<Vec<GearPage>>(),
            );
            assert_eq!(
                p1.iter_end_inclusive(p2)
                    .unwrap()
                    .collect::<Vec<GearPage>>(),
                (num1..=num2).map(GearPage::from).collect::<Vec<GearPage>>(),
            );
            assert_eq!(
                p1.iter_count(p2).unwrap().collect::<Vec<GearPage>>(),
                (num1..num1 + num2)
                    .map(GearPage::from)
                    .collect::<Vec<GearPage>>(),
            );
            assert_eq!(
                p1.iter_from_zero().collect::<Vec<GearPage>>(),
                (0..num1).map(GearPage::from).collect::<Vec<GearPage>>(),
            );
            assert_eq!(
                p1.iter_from_zero_inclusive().collect::<Vec<GearPage>>(),
                (0..=num1).map(GearPage::from).collect::<Vec<GearPage>>(),
            );
        };

        test(0, 1);
        test(111, 365);
        test(1238, 3498);
        test(0, 64444);
    }
}
