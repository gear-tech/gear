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

use core::{cmp::Ordering, num::NonZeroU32};
pub use numerated::{
    Bound, BoundValue, Drops, Interval, LowerBounded, NotEmptyInterval, Numerated, UpperBounded,
};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// A WebAssembly page has a constant size of 64KiB.
pub const WASM_PAGE_SIZE: usize = 0x10000;
/// +_+_+
pub const WASM_PAGE_SIZE32: u32 = 0x10000;

// +_+_+ change comment
/// A gear page size, currently 16KiB to fit the most common native page size.
/// This is size of memory data pages in storage.
/// So, in lazy-pages, when program tries to access some memory interval -
/// we can download just some number of gear pages instead of whole wasm page.
/// The number of small pages, which must be downloaded, is depends on host
/// native page size, so can vary.
pub const GEAR_PAGE_SIZE: usize = 0x4000;
/// +_+_+
pub const GEAR_PAGE_SIZE32: u32 = 0x4000;

static_assertions::const_assert!(WASM_PAGE_SIZE < u32::MAX as usize);
static_assertions::const_assert_eq!(WASM_PAGE_SIZE % GEAR_PAGE_SIZE, 0);

/// +_+_+
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
    TypeInfo,
    Default,
    derive_more::Into,
)]
pub struct PagesAmount<const SIZE: u32>(u32);

impl<const SIZE: u32> PagesAmount<SIZE> {
    /// Page size. May be any number power of two in interval [2, u32::MAX].
    ///
    /// NOTE: In case SIZE == 0 or 1 or any not power of two number, then you would receive compilation error.
    pub const SIZE: u32 = SIZE;

    /// Number of max pages amount. Equal to max page number + 1.
    ///
    /// NOTE: const computation contains checking in order to prevent incorrect SIZE.
    pub const UPPER: Self = Self(u32::MAX / SIZE + 1 / if SIZE.is_power_of_two() { 1 } else { 0 });

    /// +_+_+
    pub fn distance_inclusive<A: Into<Self>, B: Into<Self>>(a: A, b: B) -> Option<Self> {
        let a: Self = a.into();
        let b: Self = b.into();
        a.0.checked_sub(b.0).map(|c| Self(c + 1))
    }

    /// +_+_+
    pub fn add<A: Into<Self>, B: Into<Self>>(a: A, b: B) -> Option<Self> {
        let a: Self = a.into();
        let b: Self = b.into();
        a.0.checked_add(b.0)
            .and_then(|c| (c <= Self::UPPER.0).then_some(Self(c)))
    }

    /// +_+_+
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// +_+_+
    pub fn raw(&self) -> u32 {
        self.0
    }

    /// +_+_+ remove
    pub fn to_page(&self) -> Option<Page<SIZE>> {
        self.get()
    }

    /// +_+_+
    pub fn to_pages_amount<const S: u32>(&self) -> PagesAmount<S> {
        let raw = if Self::SIZE > S {
            (Self::SIZE / S) * self.0
        } else {
            self.0 / (S / Self::SIZE)
        };
        PagesAmount(raw)
    }
}

impl PagesAmount<WASM_PAGE_SIZE32> {
    /// +_+_+
    pub const fn from_u16(raw: u16) -> Self {
        Self(raw as u32)
    }
}

impl<const SIZE: u32> From<PagesAmount<SIZE>> for Option<u32> {
    fn from(value: PagesAmount<SIZE>) -> Option<u32> {
        match value.0 {
            a if a > PagesAmount::<SIZE>::UPPER.0 => {
                unreachable!("PageBound must be always less or equal than UPPER")
            }
            a => Some(a),
        }
    }
}

impl<const SIZE: u32> From<Page<SIZE>> for PagesAmount<SIZE> {
    fn from(value: Page<SIZE>) -> Self {
        Self(value.0)
    }
}

// impl<const SIZE: u32> From<PagesAmount<SIZE>> for Option<Page<SIZE>> {
//     fn from(value: PagesAmount<SIZE>) -> Option<Page<SIZE>> {
//     }
// }

impl<const SIZE: u32> From<Option<Page<SIZE>>> for PagesAmount<SIZE> {
    fn from(value: Option<Page<SIZE>>) -> Self {
        value.map(|page| page.into()).unwrap_or(Self::UPPER)
    }
}

impl<const SIZE: u32> Bound<Page<SIZE>> for PagesAmount<SIZE> {
    fn unbound(self) -> BoundValue<Page<SIZE>> {
        match self {
            a if a > Self::UPPER => {
                unreachable!("PageBound must be always less or equal than UPPER")
            }
            a if a == PagesAmount::<SIZE>::UPPER => BoundValue::Upper(Page::UPPER),
            a => BoundValue::Value(Page(a.0)),
        }
    }
}

impl<const SIZE: u32> TryFrom<u32> for PagesAmount<SIZE> {
    type Error = ();

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        if raw > Self::UPPER.0 {
            Err(())
        } else {
            Ok(Self(raw))
        }
    }
}

impl<const SIZE: u32> PartialEq<Page<SIZE>> for PagesAmount<SIZE> {
    fn eq(&self, other: &Page<SIZE>) -> bool {
        self.0 == other.0
    }
}

impl<const SIZE: u32> PartialOrd<Page<SIZE>> for PagesAmount<SIZE> {
    fn partial_cmp(&self, other: &Page<SIZE>) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

/// +_+_+
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, TypeInfo, Default)]
pub struct Page<const SIZE: u32>(u32);

impl<const SIZE: u32> Page<SIZE> {
    /// +_+_+
    pub const SIZE: u32 = SIZE;

    /// +_+_+
    pub const UPPER: Self = Self(u32::MAX / SIZE);

    /// +_+_+
    pub fn inc(&self) -> PagesAmount<SIZE> {
        PagesAmount(self.0 + 1)
    }

    /// +_+_+
    pub fn from_offset(offset: u32) -> Self {
        Self(offset / SIZE)
    }
}

impl<const SIZE: u32> From<Page<SIZE>> for u32 {
    fn from(value: Page<SIZE>) -> Self {
        value.0
    }
}

impl<const SIZE: u32> TryFrom<u32> for Page<SIZE> {
    // +_+_+
    type Error = ();

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        if raw >= <Self as Numerated>::B::UPPER.0 {
            Err(())
        } else {
            Ok(Self(raw))
        }
    }
}

impl<const SIZE: u32> PageNumber for Page<SIZE> {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl<const SIZE: u32> Numerated for Page<SIZE> {
    type N = u32;
    type B = PagesAmount<SIZE>;

    fn raw_add_if_lt(self, num: Self::N, other: Self) -> Option<Self> {
        (self.0.checked_add(num)? <= other.0).then_some(Self(self.0 + num))
    }

    fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self> {
        (self.0.checked_sub(num)? >= other.0).then_some(Self(self.0 - num))
    }

    fn sub(self, other: Self) -> Option<Self::N> {
        self.0.checked_sub(other.0)
    }
}

impl<const SIZE: u32> LowerBounded for Page<SIZE> {
    fn min_value() -> Self {
        Self(0)
    }
}

impl<const SIZE: u32> UpperBounded for Page<SIZE> {
    fn max_value() -> Self {
        Self(<Self as Numerated>::B::UPPER.0 - 1)
    }
}

/// +_+_+
pub type WasmPage = Page<WASM_PAGE_SIZE32>;
/// +_+_+
pub type GearPage = Page<GEAR_PAGE_SIZE32>;
/// +_+_+
pub type WasmPagesAmount = PagesAmount<WASM_PAGE_SIZE32>;
/// +_+_+
pub type GearPagesAmount = PagesAmount<GEAR_PAGE_SIZE32>;

impl From<u16> for WasmPagesAmount {
    fn from(value: u16) -> Self {
        static_assertions::const_assert!(WASM_PAGE_SIZE <= 0x10_000);
        Self(value as u32)
    }
}

impl From<u16> for GearPagesAmount {
    fn from(value: u16) -> Self {
        static_assertions::const_assert!(GEAR_PAGE_SIZE <= 0x10_000);
        Self(value as u32)
    }
}

impl From<u16> for GearPage {
    fn from(value: u16) -> Self {
        static_assertions::const_assert!(GEAR_PAGE_SIZE <= 0x10_000);
        Page(value as u32)
    }
}

impl From<u16> for WasmPage {
    fn from(value: u16) -> Self {
        static_assertions::const_assert!(WASM_PAGE_SIZE <= 0x10_000);
        Page(value as u32)
    }
}

impl PageU32Size for GearPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(GEAR_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(GEAR_PAGE_SIZE as u32) }
    }
}

impl PageU32Size for WasmPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(WASM_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(WASM_PAGE_SIZE as u32) }
    }
}

/// Page number trait - page, which can return it number as u32.
pub trait PageNumber: Numerated + Into<u32> {
    /// Creates page from raw number.
    ///
    /// # Safety
    /// This function is unsafe because it can create invalid page number.
    unsafe fn from_raw(raw: u32) -> Self;

    /// Returns raw (u32) page number.
    fn raw(&self) -> u32 {
        Into::<u32>::into(*self)
    }

    /// +_+_+
    fn is_zero(&self) -> bool {
        self.raw() == 0
    }

    // /// Returns iterator `self`..=`end`.
    // fn iter_end_inclusive(&self, end: Self) -> Option<Interval<Self>> {
    //     Interval::try_from(*self..=end).ok()
    // }

    // /// Returns iterator `0`..=`self`
    // fn iter_from_zero_inclusive(&self) -> Interval<Self> {
    //     (..=*self).into()
    // }
    // /// Returns iterator `0`..`self`
    // fn iter_from_zero(&self) -> Interval<Self> {
    //     Interval::from(..*self)
    // }
    // /// Returns iterator `self`..=`self`
    // fn iter_once(&self) -> Interval<Self> {
    //     Interval::from(self)
    // }
}

/// Trait represents page with u32 size for u32 memory: max memory size is 2^32 bytes.
/// All operations with page guarantees, that no addr or page number can be overflowed.
pub trait PageU32Size: PageNumber {
    /// Returns size of page.
    fn size_non_zero() -> NonZeroU32;
    /// Size as u32. Cannot be zero, because uses `Self::size_non_zero`.
    fn size() -> u32 {
        Self::size_non_zero().into()
    }
    /// Constructs new page from byte offset: returns page which contains this byte.
    fn from_offset(offset: u32) -> Self {
        unsafe { Self::from_raw(offset / Self::size()) }
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
    /// Aligns page zero byte and returns page which contains this byte.
    /// Normally if `size % Self::size() == 0`,
    /// then aligned byte is zero byte of the returned page.
    fn align_down(&self, size: NonZeroU32) -> Self {
        let size: u32 = size.into();
        Self::from_offset((self.offset() / size) * size)
    }
    /// Returns iterator `0`..=`self`
    fn iter_from_zero_inclusive(&self) -> PagesIterInclusive<Self> {
        PagesIterInclusive {
            page: Some(unsafe { Self::from_raw(0) }),
            end: *self,
        }
    }
    /// Returns iterator `0`..`self`
    fn iter_from_zero(&self) -> PagesIter<Self> {
        PagesIter {
            page: unsafe { Self::from_raw(0) },
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
    /// Returns an iterator that iterates over the range of pages from `self` to the end page,
    /// inclusive. Each iteration yields a page of type `P`.
    ///
    /// # Example
    ///
    /// ```
    /// use gear_core::pages::{PageU32Size, GearPage, PageNumber};
    ///
    /// let new_page = GearPage::from(5);
    ///
    /// let pages_iter = new_page.to_pages_iter::<GearPage>();
    ///
    /// for page in pages_iter {
    ///     println!("Page number: {}", page.raw());
    /// }
    /// ```
    ///
    /// # Generic Parameters
    ///
    /// - `P`: The type of pages in the iterator, which must implement the `PageU32Size` trait.
    fn to_pages_iter<P: PageU32Size>(&self) -> PagesIterInclusive<P> {
        let start: P = self.to_page();
        let end: P = P::from_offset(self.end_offset());
        PagesIterInclusive {
            page: Some(start),
            end,
        }
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
            self.page = P::from_raw(self.page.raw() + 1);
        }
        Some(res)
    }
}

/// U32 size pages iterator, to iterate continuously from one page to another, including the last one.
#[derive(Debug, Clone)]
pub struct PagesIterInclusive<P: PageNumber> {
    page: Option<P>,
    end: P,
}

impl<P: PageNumber> Iterator for PagesIterInclusive<P> {
    type Item = P;

    fn next(&mut self) -> Option<Self::Item> {
        let page = self.page?;
        match self.end.raw() {
            end if end == page.raw() => self.page = None,
            end if end > page.raw() => unsafe {
                // Safe, because we checked that `page` is less than `end`.
                self.page = Some(P::from_raw(page.raw() + 1));
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

impl<P: PageNumber> PagesIterInclusive<P> {
    /// Returns current page.
    pub fn current(&self) -> Option<P> {
        self.page
    }
    /// Returns the end page.
    pub fn end(&self) -> P {
        self.end
    }
}

// impl<P: PageU32Size> PagesIterInclusive<P> {
//     /// Converts a page iterator from one page type to another.
//     ///
//     /// Given a page iterator `iter` of type `P1`, this function returns a new page iterator
//     /// where each page in `iter` is converted to type `P2`. The resulting iterator will
//     /// iterate over pages of type `P2`.
//     ///
//     /// # Example
//     ///
//     /// Converting a `PagesIterInclusive<GearPage>` to `PagesIterInclusive<WasmPage>`:
//     ///
//     /// ```
//     /// use gear_core::pages::{PageU32Size, PagesIterInclusive, GearPage, WasmPage, PageNumber};
//     ///
//     /// let start_page = GearPage::from(5);
//     /// let end_page = GearPage::from(10);
//     ///
//     /// let gear_iter = start_page
//     ///     .iter_end_inclusive(end_page)
//     ///     .expect("cannot iterate");
//     ///
//     /// let wasm_iter = gear_iter.convert::<WasmPage>();
//     /// ```
//     ///
//     /// # Generic parameters
//     ///
//     /// - `P1`: The type of the pages to convert to.
//     ///
//     /// # Returns
//     ///
//     /// A new page iterator of type `P1`.
//     pub fn convert<P1: PageU32Size>(&self) -> PagesIterInclusive<P1> {
//         PagesIterInclusive::<P1> {
//             page: self.page.map(|p| p.to_page()),
//             end: self.end.to_last_page(),
//         }
//     }
// }
