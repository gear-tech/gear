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
    ops::{Add, Deref, DerefMut, Sub},
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
const WASM_PAGE_SIZE: usize = 0x10000;

/// A gear page size, currently 4KiB to fit the most common native page size.
/// This is size of memory data pages in storage.
/// So, in lazy-pages, when program tries to access some memory interval -
/// we can download just some number of gear pages instead of whole wasm page.
/// The number of small pages, which must be downloaded, is depends on host
/// native page size, so can vary.
const GEAR_PAGE_SIZE: usize = 0x1000;

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

/// Number of gear pages in one wasm page
const GEAR_PAGES_IN_ONE_WASM: u32 = (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32;

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

pub use gear_core_errors::MemoryError as Error;

/// Page number.
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    Encode,
    derive_more::From,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    TypeInfo,
    Default,
)]
pub struct PageNumber(pub u32);

impl PageNumber {
    /// Creates new page from raw addr - pages which contains this addr
    pub fn new_from_addr(addr: usize) -> Self {
        Self((addr / Self::size()) as u32)
    }

    /// Return page offset.
    pub fn offset(&self) -> usize {
        (self.0 as usize) * Self::size()
    }

    /// Returns wasm page number which contains this gear page.
    pub fn to_wasm_page(&self) -> WasmPageNumber {
        (self.0 / PageNumber::num_in_one_wasm_page()).into()
    }

    /// Saturating addition.
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction.
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Return page size in bytes.
    pub const fn size() -> usize {
        GEAR_PAGE_SIZE
    }

    /// Number of gear pages in one wasm page
    pub const fn num_in_one_wasm_page() -> u32 {
        GEAR_PAGES_IN_ONE_WASM
    }
}

impl Add for PageNumber {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl Sub for PageNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self(self.0 - other.0)
    }
}

/// Wasm page number.
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    Encode,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::From,
    TypeInfo,
    Default,
)]
pub struct WasmPageNumber(pub u32);

impl WasmPageNumber {
    /// Returns new from raw addr - page which contains this addr.
    pub fn new_from_addr(addr: usize) -> Self {
        Self((addr / Self::size()) as u32)
    }

    /// Returns page offset.
    pub fn offset(&self) -> usize {
        (self.0 as usize) * Self::size()
    }

    /// Amount of gear pages in current amount of wasm pages.
    /// Or the same: number of first gear page in current wasm page.
    pub fn to_gear_page(&self) -> PageNumber {
        PageNumber::from(self.0 * PageNumber::num_in_one_wasm_page())
    }

    /// Saturating addition.
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction.
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Return page size in bytes.
    pub const fn size() -> usize {
        WASM_PAGE_SIZE
    }

    /// Returns iterator over all gear pages which this wasm page contains.
    pub fn to_gear_pages_iter(&self) -> impl Iterator<Item = PageNumber> {
        let page = self.to_gear_page();
        (page.0..page.0 + PageNumber::num_in_one_wasm_page()).map(PageNumber)
    }
}

impl Add for WasmPageNumber {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl Sub for WasmPageNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self(self.0 - other.0)
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
    fn grow(&mut self, pages: WasmPageNumber) -> Result<PageNumber, Error>;

    /// Return current size of the memory.
    fn size(&self) -> WasmPageNumber;

    /// Set memory region at specific pointer.
    fn write(&mut self, offset: usize, buffer: &[u8]) -> Result<(), Error>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<(), Error>;

    /// Returns the byte length of this memory.
    fn data_size(&self) -> usize;

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
#[derive(Debug, Default)]
pub struct AllocationsContext {
    /// Pages which has been in storage before execution
    init_allocations: BTreeSet<WasmPageNumber>,
    allocations: BTreeSet<WasmPageNumber>,
    max_pages: WasmPageNumber,
    static_pages: WasmPageNumber,
}

impl Clone for AllocationsContext {
    fn clone(&self) -> Self {
        Self {
            allocations: self.allocations.clone(),
            init_allocations: self.init_allocations.clone(),
            max_pages: self.max_pages,
            static_pages: self.static_pages,
        }
    }
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
    ) -> Result<WasmPageNumber, Error> {
        let mut previous = None;
        let mut current = None;

        let mut at = None;

        let last_static_page = (self.static_pages.0 != 0).then(|| self.static_pages - 1.into());
        for page in last_static_page.iter().chain(self.allocations.iter()) {
            if current.is_some() {
                previous = current;
            }

            current = Some(page);

            if let Some(&previous) = previous {
                if (*page).saturating_sub(previous) > pages {
                    at = Some((previous).saturating_add(1.into()));
                    break;
                }
            }
        }

        let at = at
            .or_else(|| current.map(|v| (*v).saturating_add(1.into())))
            .unwrap_or(self.static_pages);

        let final_page = at.saturating_add(pages);
        if final_page > self.max_pages {
            return Err(Error::OutOfBounds);
        }

        let extra_grow = final_page.saturating_sub(mem.size());
        if extra_grow > 0.into() {
            let grow_handler = G::before_grow_action(mem);
            mem.grow(extra_grow)?;
            grow_handler.after_grow_action(mem)?;
        }

        for page_num in at.0..final_page.0 {
            self.allocations.insert(WasmPageNumber(page_num));
        }

        Ok(at)
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

    /// Return reference to the allocation manager.
    pub fn allocations(&self) -> &BTreeSet<WasmPageNumber> {
        &self.allocations
    }

    /// Returns number of static pages.
    pub fn static_pages(&self) -> WasmPageNumber {
        self.static_pages
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
    use super::{PageBuf, PageNumber, WasmPageNumber};
    use alloc::{vec, vec::Vec};

    #[test]
    /// Test that PageNumbers add up correctly
    fn page_number_addition() {
        let sum = PageNumber(100) + PageNumber(200);

        assert_eq!(sum, PageNumber(300));

        let sum = PageNumber(200) + PageNumber(100);

        assert_eq!(sum, PageNumber(300));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    /// Test that PageNumbers addition causes panic on overflow
    fn page_number_addition_with_overflow() {
        let _ = PageNumber(u32::MAX) + PageNumber(1);
    }

    #[test]
    /// Test that PageNumbers subtract correctly
    fn page_number_subtraction() {
        let subtraction = PageNumber(299) - PageNumber(199);

        assert_eq!(subtraction, PageNumber(100))
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    /// Test that PageNumbers subtraction causes panic on overflow
    fn page_number_subtraction_with_overflow() {
        let _ = PageNumber(1) - PageNumber(u32::MAX);
    }

    #[test]
    /// Test that WasmPageNumber set transforms correctly to PageNumber set.
    fn wasm_pages_to_gear_pages() {
        let wasm_pages: Vec<WasmPageNumber> =
            [0u32, 10u32].iter().copied().map(WasmPageNumber).collect();
        let gear_pages: Vec<u32> = wasm_pages
            .iter()
            .flat_map(|p| p.to_gear_pages_iter())
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
        let mut data = vec![199u8; PageNumber::size()];
        data[1] = 2;
        let page_buf = PageBuf::new_from_vec(data).unwrap();
        log::debug!("page buff = {:?}", page_buf);
    }
}
