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

//! Module for memory and allocations context.

use crate::{
    buffer::LimitedVec,
    gas::ChargeError,
    pages::{PageU32Size, WasmPage, GEAR_PAGE_SIZE},
};
use alloc::{collections::BTreeSet, format};
use byteorder::{ByteOrder, LittleEndian};
use core::{
    fmt,
    fmt::Debug,
    iter,
    ops::{Deref, DerefMut},
};
use scale_info::{
    scale::{self, Decode, Encode, EncodeLike, Input, Output},
    TypeInfo,
};

/// Interval in wasm program memory.
#[derive(Clone, Copy, Encode, Decode)]
pub struct MemoryInterval {
    /// Interval offset in bytes.
    pub offset: u32,
    /// Interval size in bytes.
    pub size: u32,
}

impl MemoryInterval {
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        LittleEndian::write_u32(&mut bytes[0..4], self.offset);
        LittleEndian::write_u32(&mut bytes[4..8], self.size);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() == 8);
        let offset = LittleEndian::read_u32(&bytes[0..4]);
        let size = LittleEndian::read_u32(&bytes[4..8]);
        MemoryInterval { offset, size }
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "PageBuf({:?}..{:?})",
            &self.0.inner()[0..10],
            &self.0.inner()[GEAR_PAGE_SIZE - 10..GEAR_PAGE_SIZE]
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

/// Host pointer type.
/// Host pointer can be 64bit or less, to support both we use u64.
pub type HostPointer = u64;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

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
    use crate::pages::{GearPage, PageNumber};

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
        data.inner_mut()[1] = 2;
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
