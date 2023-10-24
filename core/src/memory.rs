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
    pages::{
        Bound, Drops, Interval, Numerated, PageNumber, UpperBounded, WasmPage, WasmPagesAmount,
        GEAR_PAGE_SIZE,
    },
};
use alloc::format;
use byteorder::{ByteOrder, LittleEndian};
use core::{
    fmt,
    fmt::Debug,
    ops::{Deref, DerefMut, Not},
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
    allocations: Drops<WasmPage>,
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

/// Allocation error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum AllocError {
    /// Incorrect allocation data error
    #[display(fmt = "Allocated memory pages or memory size are incorrect")]
    IncorrectAllocationData,
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
        allocations: Drops<WasmPage>,
        static_pages: WasmPagesAmount,
        max_pages: WasmPagesAmount,
    ) -> Self {
        Self {
            allocations,
            max_pages,
            static_pages,
        }
    }

    /// +_+_+
    pub fn allocations(&self) -> &Drops<WasmPage> {
        &self.allocations
    }

    /// Allocates specified number of continuously going pages
    /// and returns zero-based number of the first one.
    pub fn alloc<G: GrowHandler>(
        &mut self,
        pages: WasmPagesAmount,
        mem: &mut impl Memory,
        charge_gas_for_grow: impl FnOnce(WasmPagesAmount) -> Result<(), ChargeError>,
    ) -> Result<WasmPage, AllocError> {
        // All allocations must be after static pages.
        if self
            .allocations
            .start()
            .map(|s| self.static_pages > s)
            .unwrap_or(false)
        {
            return Err(AllocError::IncorrectAllocationData);
        }

        // All allocations must be before inside allocated executor memory.
        let mem_size = mem.size();
        if self
            .allocations
            .end()
            .map(|e| mem_size <= e)
            .unwrap_or(false)
        {
            return Err(AllocError::IncorrectAllocationData);
        }

        let mut res = None;
        for v in self
            .allocations
            .try_voids((self.static_pages, mem_size))
            .map_err(|_| AllocError::IncorrectAllocationData)?
        {
            let interval = Interval::<WasmPage>::count_from(v.start(), pages)
                .ok_or(AllocError::ProgramAllocOutOfBounds)?;
            if WasmPagesAmount::from(v.size()) >= pages {
                res = Some(interval);
                break;
            }
        }

        if let Some(v) = res {
            self.allocations.insert(v);
            return Ok(v.start());
        } else if pages.is_zero() {
            return Ok(WasmPage::UPPER);
        }

        let start = self
            .allocations
            .end()
            .map_or_else(
                || self.static_pages.get(),
                |end| end.inc_if_lt(WasmPage::max_value()),
            )
            .ok_or(AllocError::ProgramAllocOutOfBounds)?;

        // Panic is impossible, because we have already checked that pages > 0
        let interval = Interval::<WasmPage>::count_from(start, pages);
        let interval = interval
            .and_then(|interval| {
                let interval = interval
                    .into_not_empty()
                    .unwrap_or_else(|| unreachable!("New allocated interval is empty"));
                (self.max_pages > interval.end()).then_some(interval)
            })
            .ok_or(AllocError::ProgramAllocOutOfBounds)?;

        // Panic is impossible, if `end` is less than `mem_size`, than it would found interval inside existing memory.
        let grow_size = WasmPagesAmount::distance_inclusive(interval.end(), mem_size)
            .and_then(|size| size.is_zero().not().then_some(size))
            .unwrap_or_else(|| {
                unreachable!("new allocated interval end must be bigger than current `mem_size`");
            });

        charge_gas_for_grow(grow_size)?;
        let grow_handler = G::before_grow_action(mem);
        mem.grow(grow_size)
            .unwrap_or_else(|err| unreachable!("Failed to grow memory: {:?}", err));
        grow_handler.after_grow_action(mem);

        self.allocations.insert(interval);

        Ok(start)
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&mut self, page: WasmPage) -> Result<(), AllocError> {
        // +_+_+ optimize
        if self.static_pages > page || self.max_pages <= page || !self.allocations.contains(page) {
            Err(AllocError::InvalidFree(page.raw()))
        } else {
            self.allocations.remove(page);
            Ok(())
        }
    }

    /// Decomposes this instance and returns allocations.
    pub fn into_parts(self) -> (WasmPagesAmount, Drops<WasmPage>) {
        (self.static_pages, self.allocations)
    }
}

#[cfg(test)]
/// This module contains tests of GearPage struct
mod tests {
    use super::*;
    use crate::pages::{GearPage, PageU32Size};
    use alloc::vec::Vec;

    // +_+_+ add test?
    #[test]
    /// Test that [WasmPage] set transforms correctly to [GearPage] set.
    fn wasm_pages_to_gear_pages() {
        let wasm_pages: Vec<WasmPage> = [0u16, 10].iter().copied().map(WasmPage::from).collect();
        let gear_pages: Vec<u32> = wasm_pages
            .iter()
            .flat_map(|p| p.to_pages_iter::<GearPage>())
            .map(|p| p.raw())
            .collect();

        let expectation = [0, 1, 2, 3, 40, 41, 42, 43];

        assert!(gear_pages.eq(&expectation));
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
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1)));

        let mut ctx = AllocationsContext::new(Default::default(), 1.into(), 0.into());
        assert_eq!(ctx.free(0.into()), Err(AllocError::InvalidFree(0)));

        let mut ctx = AllocationsContext::new(
            [WasmPage::from(0)].into_iter().collect(),
            1.into(),
            1.into(),
        );
        assert_eq!(ctx.free(1.into()), Err(AllocError::InvalidFree(1)));
    }

    // +_+_+ make tests may be for intervals
    // #[test]
    // fn page_iterator() {
    //     let test = |num1, num2| {
    //         let p1 = GearPage::from(num1);
    //         let p2 = GearPage::from(num2);

    //         assert_eq!(
    //             p1.iter_end(p2).unwrap().collect::<Vec<GearPage>>(),
    //             (num1..num2).map(GearPage::from).collect::<Vec<GearPage>>(),
    //         );
    //         assert_eq!(
    //             p1.iter_end_inclusive(p2)
    //                 .unwrap()
    //                 .collect::<Vec<GearPage>>(),
    //             (num1..=num2).map(GearPage::from).collect::<Vec<GearPage>>(),
    //         );
    //         assert_eq!(
    //             p1.iter_count(p2).unwrap().collect::<Vec<GearPage>>(),
    //             (num1..num1 + num2)
    //                 .map(GearPage::from)
    //                 .collect::<Vec<GearPage>>(),
    //         );
    //         assert_eq!(
    //             p1.iter_from_zero().collect::<Vec<GearPage>>(),
    //             (0..num1).map(GearPage::from).collect::<Vec<GearPage>>(),
    //         );
    //         assert_eq!(
    //             p1.iter_from_zero_inclusive().collect::<Vec<GearPage>>(),
    //             (0..=num1).map(GearPage::from).collect::<Vec<GearPage>>(),
    //         );
    //     };

    //     test(0, 1);
    //     test(111, 365);
    //     test(1238, 3498);
    //     test(0, 64444);
    // }

    mod property_tests {
        use super::*;
        use crate::memory::HostPointer;
        use proptest::{
            arbitrary::any,
            collection::size_range,
            prop_oneof, proptest,
            strategy::{Just, Strategy},
            test_runner::Config as ProptestConfig,
        };

        struct TestMemory(WasmPagesAmount);

        impl Memory for TestMemory {
            type GrowError = ();

            fn grow(&mut self, pages: WasmPagesAmount) -> Result<(), Self::GrowError> {
                self.0 = WasmPagesAmount::add(self.0, pages).ok_or(())?;
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

        #[derive(Debug, Clone)]
        enum Action {
            // +_+_+ change to WasmPagesAmount
            Alloc { pages: WasmPage },
            Free { page: WasmPage },
        }

        fn actions() -> impl Strategy<Value = Vec<Action>> {
            let action = wasm_page().prop_flat_map(|page| {
                prop_oneof![
                    Just(Action::Alloc { pages: page }),
                    Just(Action::Free { page })
                ]
            });
            proptest::collection::vec(action, 0..1024)
        }

        fn allocations() -> impl Strategy<Value = Drops<WasmPage>> {
            proptest::collection::vec(wasm_page(), size_range(0..2048))
                .prop_map(|pages| pages.into_iter().collect::<Drops<WasmPage>>())
        }

        fn wasm_page() -> impl Strategy<Value = WasmPage> {
            any::<u16>().prop_map(WasmPage::from)
        }

        fn wasm_page_bound() -> impl Strategy<Value = WasmPagesAmount> {
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
        fn assert_free_error(err: AllocError) {
            match err {
                AllocError::InvalidFree(_) => {}
                err => panic!("{err:?}"),
            }
        }

        // #[test]
        // fn lol() {
        //     let _ = env_logger::try_init();

        //     let static_pages = 0.into();
        //     let allocations = Default::default(); // [WasmPage::from(65535)].into_iter().collect();
        //     let max_pages = 91.into();
        //     let mem_size = 584.into();

        //     let mut ctx = AllocationsContext::new(allocations, static_pages, max_pages);
        //     let mut mem = TestMemory(mem_size);
        //     ctx.alloc::<NoopGrowHandler>(92.into(), &mut mem, |_| Ok(())).unwrap();
        //     log::trace!("{:?}", ctx.allocations);
        //     ctx.free(65.into()).unwrap();
        //     log::trace!("{:?}", ctx.allocations);
        //     ctx.free(43.into()).unwrap();
        //     log::trace!("{:?}", ctx.allocations);
        //     ctx.free(90.into()).unwrap();
        //     log::trace!("{:?}", ctx.allocations);
        //     ctx.alloc::<NoopGrowHandler>(27294.into(), &mut mem, |_| Ok(())).expect_err("LOL");
        // }

        proptest! {
            #![proptest_config(proptest_config())]
            #[test]
            fn alloc(
                static_pages in wasm_page_bound(),
                allocations in allocations(),
                max_pages in wasm_page_bound(),
                mem_size in wasm_page_bound(),
                actions in actions(),
            ) {
                let _ = env_logger::try_init();

                let mut ctx = AllocationsContext::new(allocations, static_pages, max_pages);
                let mut mem = TestMemory(mem_size);

                for action in actions {
                    match action {
                        Action::Alloc { pages } => {
                            match ctx.alloc::<NoopGrowHandler>(pages.into(), &mut mem, |_| Ok(())) {
                                Err(AllocError::IncorrectAllocationData) => {
                                    assert!(
                                        static_pages > mem_size
                                            || ctx.allocations.end().and_then(|e| (mem.size() <= e).then_some(())).is_some()
                                            || ctx.allocations.start().and_then(|s| (static_pages > s).then_some(())).is_some()
                                    );
                                }
                                Err(AllocError::ProgramAllocOutOfBounds) => {
                                    let x = WasmPagesAmount::add(mem.size(), pages);
                                    // assert!(x.is_none() || x.unwrap() > max_pages, "{:?} {pages:?} {max_pages:?} {static_pages:?} {:?}", mem.size(), ctx.allocations);
                                    assert!(x.is_none() || x.unwrap() > max_pages);
                                }
                                Err(err) => panic!("{err:?}"),
                                Ok(_) => {}
                            }
                        }
                        Action::Free { page } => {
                            if let Err(err) = ctx.free(page) {
                                assert_free_error(err);
                            }
                        }
                    }
                }
            }
        }
    }
}
