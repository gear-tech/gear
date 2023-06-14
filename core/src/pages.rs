// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Module for memory pages.

use core::num::NonZeroU32;
use scale_info::{
    scale::{Decode, Encode},
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

/// Page number.
#[derive(
    Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, Hash, TypeInfo, Default,
)]
pub struct GearPage(pub(crate) u32);

impl From<u16> for GearPage {
    fn from(value: u16) -> Self {
        // u16::MAX * GearPage::size() - 1 <= u32::MAX
        static_assertions::const_assert!(GEAR_PAGE_SIZE <= 0x10000);
        GearPage(value as u32)
    }
}

impl From<GearPage> for u32 {
    fn from(value: GearPage) -> Self {
        value.0
    }
}

/// Wasm page number.
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, TypeInfo, Default)]
pub struct WasmPage(pub(crate) u32);

impl From<u16> for WasmPage {
    fn from(value: u16) -> Self {
        // u16::MAX * WasmPage::size() - 1 == u32::MAX
        static_assertions::const_assert!(WASM_PAGE_SIZE == 0x10000);
        WasmPage(value as u32)
    }
}

impl From<WasmPage> for u32 {
    fn from(value: WasmPage) -> Self {
        value.0
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

/// Context where dynamic size pages store their sizes
pub trait SizeManager {
    /// Returns non-zero size of page.
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32;
    /// Returns size of page.
    fn size<P: PageDynSize>(&self) -> u32 {
        self.size_non_zero::<P>().into()
    }
}

/// Page number trait - page, which can return it number as u32.
pub trait PageNumber: Into<u32> + Sized + Copy + Clone + PageU32Size {
    /// Creates page from raw number.
    ///
    /// # Safety
    /// This function is unsafe because it can create invalid page number.
    unsafe fn from_raw(raw: u32) -> Self;

    /// Returns raw (u32) page number.
    fn raw(&self) -> u32 {
        Into::<u32>::into(*self)
    }

    /// Checked subtraction.
    fn checked_sub(&self, other: Self) -> Option<Self> {
        PageNumber::raw(self)
            .checked_sub(PageNumber::raw(&other))
            .map(|p| unsafe { Self::from_raw(p) })
    }

    /// Returns iterator `self`..=`end`.
    fn iter_end_inclusive(&self, end: Self) -> Option<PagesIterInclusive<Self>> {
        (PageNumber::raw(&end) >= PageNumber::raw(self)).then_some(PagesIterInclusive {
            page: Some(*self),
            end,
        })
    }
}

/// Page with dynamic size.
pub trait PageDynSize: PageNumber {
    /// Returns size number of page.
    const SIZE_NO: usize;

    /// Returns size of page.
    fn size<S: SizeManager>(ctx: &S) -> u32 {
        ctx.size::<Self>()
    }

    /// Creates page from raw number with specific context and checks that page number is valid.
    fn new<S: SizeManager>(raw: u32, ctx: &S) -> Option<Self> {
        let page_size = <Self as PageDynSize>::size(ctx);
        let page_begin = raw.checked_mul(page_size)?;

        // Check that the last page byte has index less or equal then u32::MAX
        let last_byte_offset = page_size - 1;
        page_begin.checked_add(last_byte_offset)?;

        Some(unsafe { Self::from_raw(raw) })
    }

    /// Returns offset of page.
    fn offset<S: SizeManager>(&self, ctx: &S) -> u32 {
        PageNumber::raw(self) * <Self as PageDynSize>::size(ctx)
    }

    /// Returns offset of end of page.
    fn end_offset<S: SizeManager>(&self, ctx: &S) -> u32 {
        let size = <Self as PageDynSize>::size(ctx);
        PageNumber::raw(self) * size + (size - 1)
    }

    /// Creates page from offset.
    fn from_offset<S: SizeManager>(ctx: &S, offset: u32) -> Self {
        unsafe { Self::from_raw(offset / <Self as PageDynSize>::size(ctx)) }
    }
}

/// An enum which distinguishes between different page sizes.
pub enum PageSizeNo {
    /// Wasm page.
    WasmSizeNo = 0,
    /// Gear page.
    GearSizeNo = 1,
    /// Amount of page sizes.
    Amount = 2,
}

impl PageNumber for WasmPage {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl PageDynSize for WasmPage {
    const SIZE_NO: usize = PageSizeNo::WasmSizeNo as usize;
}

impl PageNumber for GearPage {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl PageDynSize for GearPage {
    const SIZE_NO: usize = PageSizeNo::GearSizeNo as usize;
}

#[cfg(test)]
impl SizeManager for u32 {
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32 {
        NonZeroU32::new(*self).expect("Size cannot be zero")
    }
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
