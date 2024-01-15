// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use core::cmp::Ordering;
pub use numerated::{
    num_traits::bounds::{LowerBounded, UpperBounded},
    Bound, Interval, IntervalIterator, IntervalsTree, Numerated,
};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// A WebAssembly page has a constant size of 64KiB.
const WASM_PAGE_SIZE: usize = 64 * 1024;

/// A size of memory pages in program data storage.
/// If program changes some memory page during execution, then page of this size will be uploaded to the storage.
/// If during execution program accesses some data in memory, then data of this size will be downloaded from the storage.
/// Currently equal to 16KiB to be bigger than most common host page sizes.
const GEAR_PAGE_SIZE: usize = 16 * 1024;

const _: () = assert!(WASM_PAGE_SIZE < u32::MAX as usize);
const _: () = assert!(WASM_PAGE_SIZE % GEAR_PAGE_SIZE == 0);

/// Struct represents memory pages amount with some constant size `SIZE` in bytes.
/// - `SIZE` type is u32, so page size < 4GiB (wasm32 memory size limit).
/// - `SIZE` must be power of two and must not be equal to one or zero bytes.
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

    /// Pages amount addition. Returns None if overflow.
    #[cfg(test)]
    pub fn add<A: Into<Self>, B: Into<Self>>(a: A, b: B) -> Option<Self> {
        let a: Self = a.into();
        let b: Self = b.into();
        a.0.checked_add(b.0)
            .and_then(|c| (c <= Self::UPPER.0).then_some(Self(c)))
    }

    /// Get page number, which bounds this pages amount.
    /// If pages amount == 4GB size, then returns None, because such page number does not exist.
    pub fn to_page_number(&self) -> Option<Page<SIZE>> {
        self.unbound()
    }

    /// Converts one page size to another.
    pub fn to_pages_amount<const S: u32>(&self) -> PagesAmount<S> {
        let raw = if Self::SIZE > S {
            (Self::SIZE / S) * self.0
        } else {
            self.0 / (S / Self::SIZE)
        };
        PagesAmount(raw)
    }
}

impl<const SIZE: u32> PageNumber for PagesAmount<SIZE> {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl<const SIZE: u32> From<Page<SIZE>> for PagesAmount<SIZE> {
    fn from(value: Page<SIZE>) -> Self {
        Self(value.0)
    }
}

impl<const SIZE: u32> From<Option<Page<SIZE>>> for PagesAmount<SIZE> {
    fn from(value: Option<Page<SIZE>>) -> Self {
        value.map(|page| page.into()).unwrap_or(Self::UPPER)
    }
}

impl<const SIZE: u32> Bound<Page<SIZE>> for PagesAmount<SIZE> {
    fn unbound(self) -> Option<Page<SIZE>> {
        match self {
            a if a > Self::UPPER => {
                // This panic is impossible because of `PagesAmount` constructors implementation.
                unreachable!("PageBound must be always less or equal than UPPER")
            }
            a if a == Self::UPPER => None,
            a => Some(Page(a.0)),
        }
    }
}

/// Try from u32 error for [PagesAmount].
#[derive(Debug, Clone, derive_more::Display)]
#[display(fmt = "Tries to make pages amount from {_0}, which must be less or equal to {_1}")]
pub struct PagesAmountError(u32, u32);

impl<const SIZE: u32> TryFrom<u32> for PagesAmount<SIZE> {
    type Error = PagesAmountError;

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        if raw > Self::UPPER.0 {
            Err(PagesAmountError(raw, Self::UPPER.0))
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

/// Struct represents memory page number with some constant size `SIZE` in bytes.
/// - `SIZE` type is u32, so page size < 4GiB (wasm32 memory size limit).
/// - `SIZE` must be power of two and must not be equal to zero bytes.
/// - `SIZE == 1` is possible, but then you cannot use [PagesAmount] for these pages.
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
pub struct Page<const SIZE: u32>(u32);

impl<const SIZE: u32> Page<SIZE> {
    /// Page size. May be any number power of two in interval [1, u32::MAX].
    pub const SIZE: u32 = SIZE;

    /// Max possible page number in 4GB memory.
    ///
    /// Note: const computation contains checking in order to prevent incorrect SIZE.
    #[allow(clippy::erasing_op)]
    pub const UPPER: Self = Self(u32::MAX / SIZE + 0 / if SIZE.is_power_of_two() { 1 } else { 0 });

    /// Increment page number. Returns `PagesAmount<SIZE>`, because this allows to avoid overflows.
    pub fn inc(&self) -> PagesAmount<SIZE> {
        PagesAmount(self.0 + 1)
    }

    /// Constructs new page from byte offset: returns page which contains this byte.
    pub fn from_offset(offset: u32) -> Self {
        // Safe, cause offset is always less or equal to u32::MAX.
        unsafe { Self::from_raw(offset / Self::SIZE) }
    }

    /// Returns page zero byte offset.
    pub fn offset(&self) -> u32 {
        self.raw() * Self::SIZE
    }

    /// Returns page last byte offset.
    pub fn end_offset(&self) -> u32 {
        self.raw() * Self::SIZE + (Self::SIZE - 1)
    }

    /// Returns new page, which contains `self` zero byte.
    pub fn to_page<const S1: u32>(self) -> Page<S1> {
        Page::from_offset(self.offset())
    }

    /// Returns an iterator that iterates over the range of pages from `self` to the end page,
    /// inclusive. Each iteration yields a page of type [`Page<S1>`].
    ///
    /// # Example
    ///
    /// ```
    /// use gear_core::pages::{GearPage, WasmPage};
    ///
    /// let x: Vec<GearPage> = WasmPage::from(5).to_iter().collect();
    /// println!("{x:?}");
    /// ```
    /// For this example must be printed: `[GearPage(20), GearPage(21), GearPage(22), GearPage(23)]`
    pub fn to_iter<const S1: u32>(self) -> IntervalIterator<Page<S1>> {
        let start = Page::<S1>::from_offset(self.offset());
        let end = Page::<S1>::from_offset(self.end_offset());
        // Safe, cause end byte offset is always greater or equal to offset, so `start <= end`.
        unsafe { Interval::new_unchecked(start, end).iter() }
    }
}

/// Try from u32 error for [Page].
#[derive(Debug, Clone, derive_more::Display)]
#[display(fmt = "Tries to make page from {_0}, which must be less or equal to {_1}")]
pub struct PageError(u32, u32);

impl<const SIZE: u32> TryFrom<u32> for Page<SIZE> {
    type Error = PageError;

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        if raw > Self::UPPER.0 {
            Err(PageError(raw, Self::UPPER.0))
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
    type Distance = u32;
    type Bound = PagesAmount<SIZE>;

    fn add_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
        self.0
            .checked_add(num)
            .and_then(|sum| sum.enclosed_by(&self.0, &other.0).then_some(Self(sum)))
    }

    fn sub_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
        self.0
            .checked_sub(num)
            .and_then(|sub| sub.enclosed_by(&self.0, &other.0).then_some(Self(sub)))
    }

    fn distance(self, other: Self) -> Self::Distance {
        self.0.abs_diff(other.0)
    }
}

impl<const SIZE: u32> LowerBounded for Page<SIZE> {
    fn min_value() -> Self {
        Self(0)
    }
}

impl<const SIZE: u32> UpperBounded for Page<SIZE> {
    fn max_value() -> Self {
        Self::UPPER
    }
}

/// Page of wasm page size - 64 kiB.
pub type WasmPage = Page<{ WASM_PAGE_SIZE as u32 }>;
/// Page of gear page size - 16 kiB.
pub type GearPage = Page<{ GEAR_PAGE_SIZE as u32 }>;
/// Pages amount for wasm page size - 64 kiB.
pub type WasmPagesAmount = PagesAmount<{ WASM_PAGE_SIZE as u32 }>;
/// Pages amount for gear page size - 16 kiB.
pub type GearPagesAmount = PagesAmount<{ GEAR_PAGE_SIZE as u32 }>;

impl WasmPagesAmount {
    /// Make wasm pages amount constant from u16.
    pub const fn from_u16(raw: u16) -> Self {
        static_assertions::const_assert!(WASM_PAGE_SIZE <= 0x10_000);
        Self(raw as u32)
    }
}

impl From<u16> for WasmPagesAmount {
    fn from(value: u16) -> Self {
        const _: () = assert!(WASM_PAGE_SIZE <= 0x10_000);
        Self(value as u32)
    }
}

impl From<u16> for WasmPage {
    fn from(value: u16) -> Self {
        const _: () = assert!(WASM_PAGE_SIZE <= 0x10_000);
        WasmPage(value as u32)
    }
}

impl From<u16> for GearPagesAmount {
    fn from(value: u16) -> Self {
        const _: () = assert!(GEAR_PAGE_SIZE <= 0x10_000);
        Self(value as u32)
    }
}

impl From<u16> for GearPage {
    fn from(value: u16) -> Self {
        const _: () = assert!(GEAR_PAGE_SIZE <= 0x10_000);
        Page(value as u32)
    }
}

/// Page number trait - page, which can return it number as u32.
pub trait PageNumber: Copy + Into<u32> {
    /// Creates page from raw number.
    ///
    /// # Safety
    /// This function is unsafe because it can create invalid page number.
    unsafe fn from_raw(raw: u32) -> Self;

    /// Returns raw (u32) page number.
    fn raw(&self) -> u32 {
        Into::<u32>::into(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numerated::mock::test_numerated;
    use proptest::{proptest, strategy::Strategy, test_runner::Config as ProptestConfig};

    fn rand_gear_page() -> impl Strategy<Value = GearPage> {
        (0..=GearPage::UPPER.raw()).prop_map(Page)
    }

    fn rand_wasm_page() -> impl Strategy<Value = WasmPage> {
        (0..=WasmPage::UPPER.raw()).prop_map(Page)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn proptest_gear_page_numerated(p1 in rand_gear_page(), p2 in rand_gear_page()) {
            test_numerated(p1, p2);
        }

        #[test]
        fn proptest_wasm_page_numerated(p1 in rand_wasm_page(), p2 in rand_wasm_page()) {
            test_numerated(p1, p2);
        }
    }
}
