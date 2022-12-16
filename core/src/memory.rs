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

use core::{
    convert::TryFrom,
    fmt::Debug,
    num::NonZeroU32,
    ops::{Deref, DerefMut},
};

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use codec::{Decode, Encode};
use core::fmt;
use scale_info::TypeInfo;

/// A WebAssembly page has a constant size of 64KiB.
pub const WASM_PAGE_SIZE: usize = 0x10000;

/// A gear page size, currently 4KiB to fit the most common native page size.
/// This is size of memory data pages in storage.
/// So, in lazy-pages, when program tries to access some memory interval -
/// we can download just some number of gear pages instead of whole wasm page.
/// The number of small pages, which must be downloaded, is depends on host
/// native page size, so can vary.
pub const GEAR_PAGE_SIZE: usize = 0x1000;

/// Pages data storage granularity (PSG) is a size and wasm addr alignment
/// of a memory interval, for which the following conditions must be met:
/// if some gear page has data in storage, then all gear
/// pages, that are in the same granularity interval, must contain
/// data in storage. For example:
/// ````ignored
///   granularity interval no.0       interval no.1        interval no.2
///                |                    |                    |
///                {====|====|====|====}{====|====|====|====}{====|====|====|====}
///               /     |     \
///    gear-page 0    page 1   page 2 ...
/// ````
/// In this example each PSG page contains 4 gear-pages. So, if gear-page `2`
/// has data in storage, then gear-page `0`,`1`,`3` also has data in storage.
/// This constant is necessary for consensus between nodes with different
/// native page sizes. You can see an example of using in crate `gear-lazy-pages`.
pub const PAGE_STORAGE_GRANULARITY: usize = 0x4000;

static_assertions::const_assert_eq!(WASM_PAGE_SIZE % GEAR_PAGE_SIZE, 0);
static_assertions::const_assert_eq!(WASM_PAGE_SIZE % PAGE_STORAGE_GRANULARITY, 0);
static_assertions::const_assert_eq!(PAGE_STORAGE_GRANULARITY % GEAR_PAGE_SIZE, 0);

/// Buffer for gear page data.
#[derive(Clone, Encode, Decode, PartialEq, Eq)]
pub struct PageBuf(Box<[u8; GEAR_PAGE_SIZE]>);

impl fmt::Debug for PageBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "PageBuf({:?}..{:?})",
            &self.0[0..10],
            &self.0[GEAR_PAGE_SIZE - 10..GEAR_PAGE_SIZE]
        )
    }
}

impl Deref for PageBuf {
    type Target = Box<[u8; GEAR_PAGE_SIZE]>;
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
    /// Tries to transform vec<u8> into page buffer.
    /// Makes it without any reallocations or memcpy: vector's buffer becomes PageBuf without any changes,
    /// except vector's buffer capacity, which is removed.
    pub fn new_from_vec(v: Vec<u8>) -> Result<Self, Error> {
        Box::<[u8; GEAR_PAGE_SIZE]>::try_from(v.into_boxed_slice())
            .map_err(|data| Error::InvalidPageDataSize(data.len() as u64))
            .map(Self)
    }

    /// Returns new page buffer with zeroed data.
    pub fn new_zeroed() -> PageBuf {
        Self(Box::<[u8; GEAR_PAGE_SIZE]>::new([0u8; GEAR_PAGE_SIZE]))
    }

    /// Convert page buffer into vector without reallocations.
    pub fn into_vec(self) -> Vec<u8> {
        (self.0 as Box<[_]>).into_vec()
    }
}

/// Tries to convert vector data map to page buffer data map.
/// Makes it without buffer reallocations.
pub fn vec_page_data_map_to_page_buf_map(
    pages_data: BTreeMap<PageNumber, Vec<u8>>,
) -> Result<BTreeMap<PageNumber, PageBuf>, Error> {
    let mut pages_data_res = BTreeMap::new();
    for (page, data) in pages_data {
        let data = PageBuf::new_from_vec(data)?;
        pages_data_res.insert(page, data);
    }
    Ok(pages_data_res)
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
    /// TODO
    #[display(fmt = "Cannot make pages range from {_0} to {_1} exclusive")]
    WrongRange(u32, u32),
}

/// TODO
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

/// TODO
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
    /// TODO
    pub fn current(&self) -> Option<P> {
        self.page
    }
    /// TODO
    pub fn end(&self) -> P {
        self.end
    }
}

/// Trait represents page with u32 size for u32 memory: max memory size is 2^32 bytes.
/// All operations with page guarantees, that no addr or page number can be overflowed.
pub trait PageU32Size: Sized + Clone + Copy + PartialEq + Eq {
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
    /// Returns new page, which impls PageU32Size, and contains self zero byte.
    fn to_page<PAGE: PageU32Size>(&self) -> PAGE {
        PAGE::from_offset(self.offset())
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
    /// TODO
    fn iter_count(&self, count: Self) -> Result<PagesIter<Self>, PageError> {
        self.add(count).map(|end| PagesIter { page: *self, end })
    }
    /// TODO
    fn iter_end(&self, end: Self) -> Result<PagesIter<Self>, PageError> {
        if end.raw() >= self.raw() {
            Ok(PagesIter { page: *self, end })
        } else {
            Err(PageError::WrongRange(self.raw(), end.raw()))
        }
    }
    /// TODO
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
    /// TODO
    fn iter_from_zero_inclusive(&self) -> PagesIterInclusive<Self> {
        PagesIterInclusive {
            page: Some(Self::zero()),
            end: *self,
        }
    }
    /// TODO
    fn iter_from_zero(&self) -> PagesIter<Self> {
        PagesIter {
            page: Self::zero(),
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

pub use gear_core_errors::MemoryError as Error;

/// Page number.
#[derive(
    Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, Hash, TypeInfo, Default,
)]
pub struct PageNumber(u32);

impl PageU32Size for PageNumber {
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
pub struct WasmPageNumber(u32);

impl PageU32Size for WasmPageNumber {
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

/// Page with size [PAGE_STORAGE_GRANULARITY].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GranularityPage(u32);

impl PageU32Size for GranularityPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(PAGE_STORAGE_GRANULARITY, 0);
        unsafe { NonZeroU32::new_unchecked(PAGE_STORAGE_GRANULARITY as u32) }
    }

    fn raw(&self) -> u32 {
        self.0
    }

    unsafe fn new_unchecked(num: u32) -> Self {
        Self(num)
    }
}

impl From<u16> for WasmPageNumber {
    fn from(value: u16) -> Self {
        // u16::MAX * WasmPageNumber::size() - 1 == u32::MAX
        static_assertions::const_assert!(WASM_PAGE_SIZE == 0x10000);
        WasmPageNumber(value as u32)
    }
}

impl From<u16> for PageNumber {
    fn from(value: u16) -> Self {
        // u16::MAX * PageNumber::size() - 1 <= u32::MAX
        static_assertions::const_assert!(GEAR_PAGE_SIZE <= 0x10000);
        PageNumber(value as u32)
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
    /// Grow memory by number of pages.
    fn grow(&mut self, pages: WasmPageNumber) -> Result<(), Error>;

    /// Return current size of the memory.
    fn size(&self) -> WasmPageNumber;

    /// Set memory region at specific pointer.
    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), Error>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), Error>;

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
    init_allocations: BTreeSet<WasmPageNumber>,
    allocations: BTreeSet<WasmPageNumber>,
    max_pages: WasmPageNumber,
    static_pages: WasmPageNumber,
}

/// Before and after memory grow actions.
pub trait GrowHandler {
    /// Before grow action
    fn before_grow_action(mem: &mut impl Memory) -> Self;
    /// After grow action
    fn after_grow_action(self, mem: &mut impl Memory) -> Result<(), Error>;
}

/// Grow handler do nothing implementation
pub struct GrowHandlerNothing;

impl GrowHandler for GrowHandlerNothing {
    fn before_grow_action(_mem: &mut impl Memory) -> Self {
        GrowHandlerNothing
    }
    fn after_grow_action(self, _mem: &mut impl Memory) -> Result<(), Error> {
        Ok(())
    }
}

/// Alloc method result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocInfo {
    /// Zero page of allocated interval.
    pub page: WasmPageNumber,
    /// Number of pages, which has been allocated inside already existing memory.
    pub not_grown: WasmPageNumber,
}

impl AllocationsContext {
    /// New allocations context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    pub fn new(
        allocations: BTreeSet<WasmPageNumber>,
        static_pages: WasmPageNumber,
        max_pages: WasmPageNumber,
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
    pub fn is_init_page(&self, page: WasmPageNumber) -> bool {
        self.init_allocations.contains(&page)
    }

    /// Alloc specific number of pages for the currently running program.
    pub fn alloc<G: GrowHandler>(
        &mut self,
        pages: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<AllocInfo, Error> {
        let mem_size = mem.size();
        let mut previous = self.static_pages;
        let mut start = None;
        for &page in self.allocations.iter().chain(core::iter::once(&mem_size)) {
            if page
                .sub(previous)
                .map_err(|_| Error::IncorrectAllocationsSetOrMemSize)?
                >= pages
            {
                start = Some(previous);
                break;
            }

            previous = page.inc().map_err(|_| Error::OutOfBounds)?;
        }

        let (start, not_grown) = if let Some(start) = start {
            (start, pages)
        } else {
            // If we cannot find interval between already allocated pages, then try to alloc new pages.

            // Panic is safe, because we check, that last allocated page can be incremented in loop above.
            let start = self
                .allocations
                .last()
                .map(|last| last.inc().unwrap_or_else(|err| {
                    unreachable!("Cannot increment last allocation: {}, but we checked in loop above that it can be done", err)
                }))
                .unwrap_or(self.static_pages);
            let end = start.add(pages).map_err(|_| Error::OutOfBounds)?;
            if end > self.max_pages {
                return Err(Error::OutOfBounds);
            }

            // Panic is safe, because in loop above we checked it.
            let extra_grow = end.sub(mem_size).unwrap_or_else(|err| {
                unreachable!(
                    "`mem_size` must be bigger than all allocations or static pages, but get {}",
                    err
                )
            });

            // Panic is safe, in other case we would found interval inside existing memory.
            if extra_grow == WasmPageNumber::zero() {
                unreachable!("`extra grow cannot be zero");
            }

            let grow_handler = G::before_grow_action(mem);
            mem.grow(extra_grow)?;
            grow_handler.after_grow_action(mem)?;

            // Panic is safe, because of way `extra_grow` was calculated.
            let not_grown = pages.sub(extra_grow).unwrap_or_else(|err| {
                unreachable!(
                    "`extra_grow` cannot be bigger than `pages`, but get {}",
                    err
                )
            });

            (start, not_grown)
        };

        // Panic is safe, because we calculated `start` suitable for `pages`.
        let new_allocations = start
            .iter_count(pages)
            .unwrap_or_else(|err| unreachable!("`start` + `pages` is out of wasm memory: {}", err));

        self.allocations.extend(new_allocations);

        Ok(AllocInfo {
            page: start,
            not_grown,
        })
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&mut self, page: WasmPageNumber) -> Result<(), Error> {
        if page > self.max_pages {
            Err(Error::OutOfBounds)
        } else if page < self.static_pages || !self.allocations.remove(&page) {
            Err(Error::InvalidFree(page.0))
        } else {
            Ok(())
        }
    }

    /// Decomposes this instance and returns allocations.
    pub fn into_parts(
        self,
    ) -> (
        WasmPageNumber,
        BTreeSet<WasmPageNumber>,
        BTreeSet<WasmPageNumber>,
    ) {
        (self.static_pages, self.init_allocations, self.allocations)
    }
}

#[cfg(test)]
/// This module contains tests of PageNumber struct
mod tests {
    use super::*;

    use alloc::{vec, vec::Vec};

    #[test]
    /// Test that GearPages add up correctly
    fn page_number_addition() {
        let sum = PageNumber(100).add(200.into()).unwrap();
        assert_eq!(sum, PageNumber(300));
    }

    #[test]
    /// Test that GearPages subtract correctly
    fn page_number_subtraction() {
        let subtraction = PageNumber(299).sub(199.into()).unwrap();
        assert_eq!(subtraction, PageNumber(100))
    }

    #[test]
    /// Test that WasmPageNumber set transforms correctly to PageNumber set.
    fn wasm_pages_to_gear_pages() {
        let wasm_pages: Vec<WasmPageNumber> =
            [0u32, 10u32].iter().copied().map(WasmPageNumber).collect();
        let gear_pages: Vec<u32> = wasm_pages
            .iter()
            .flat_map(|p| p.to_pages_iter::<PageNumber>())
            .map(|p| p.0)
            .collect();

        let expectation = [
            0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 160, 161, 162, 163, 164, 165,
            166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
        ];

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
        let mut data = vec![199u8; PageNumber::size() as usize];
        data[1] = 2;
        let page_buf = PageBuf::new_from_vec(data).unwrap();
        log::debug!("page buff = {:?}", page_buf);
    }

    #[test]
    fn free_fails() {
        let mut ctx =
            AllocationsContext::new(BTreeSet::default(), WasmPageNumber(0), WasmPageNumber(0));
        assert_eq!(ctx.free(WasmPageNumber(1)), Err(Error::OutOfBounds));

        let mut ctx =
            AllocationsContext::new(BTreeSet::default(), WasmPageNumber(1), WasmPageNumber(0));
        assert_eq!(ctx.free(WasmPageNumber(0)), Err(Error::InvalidFree(0)));

        let mut ctx = AllocationsContext::new(
            BTreeSet::from([WasmPageNumber(0)]),
            WasmPageNumber(1),
            WasmPageNumber(1),
        );
        assert_eq!(ctx.free(WasmPageNumber(1)), Err(Error::InvalidFree(1)));
    }

    #[test]
    fn page_iterator() {
        let test = |num1, num2| {
            let p1 = PageNumber::from(num1);
            let p2 = PageNumber::from(num2);

            assert_eq!(
                p1.iter_end(p2).unwrap().collect::<Vec<PageNumber>>(),
                (num1..num2)
                    .map(PageNumber::from)
                    .collect::<Vec<PageNumber>>(),
            );
            assert_eq!(
                p1.iter_end_inclusive(p2)
                    .unwrap()
                    .collect::<Vec<PageNumber>>(),
                (num1..=num2)
                    .map(PageNumber::from)
                    .collect::<Vec<PageNumber>>(),
            );
            assert_eq!(
                p1.iter_count(p2).unwrap().collect::<Vec<PageNumber>>(),
                (num1..num1 + num2)
                    .map(PageNumber::from)
                    .collect::<Vec<PageNumber>>(),
            );
            assert_eq!(
                p1.iter_from_zero().collect::<Vec<PageNumber>>(),
                (0..num1).map(PageNumber::from).collect::<Vec<PageNumber>>(),
            );
            assert_eq!(
                p1.iter_from_zero_inclusive().collect::<Vec<PageNumber>>(),
                (0..=num1)
                    .map(PageNumber::from)
                    .collect::<Vec<PageNumber>>(),
            );
        };

        test(0, 1);
        test(111, 365);
        test(1238, 3498);
        test(0, 64444);
    }
}
