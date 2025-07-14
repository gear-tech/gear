// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use alloc::format;
use core::cmp::Ordering;
use num_traits::bounds::{LowerBounded, UpperBounded};
use numerated::{interval::Interval, iterators::IntervalIterator, Bound, Numerated};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

pub use numerated::{self, num_traits};

/// A WebAssembly page has a constant size of 64KiB.
const WASM_PAGE_SIZE: u32 = 64 * 1024;

/// A size of memory pages in program data storage.
/// If program changes some memory page during execution, then page of this size will be uploaded to the storage.
/// If during execution program accesses some data in memory, then data of this size will be downloaded from the storage.
/// Currently equal to 16KiB to be bigger than most common host page sizes.
const GEAR_PAGE_SIZE: u32 = 16 * 1024;

const _: () = assert!(WASM_PAGE_SIZE.is_multiple_of(GEAR_PAGE_SIZE));

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
    pub fn add(&self, other: Self) -> Option<Self> {
        self.0
            .checked_add(other.0)
            .and_then(|r| (r <= Self::UPPER.0).then_some(Self(r)))
    }

    /// Get page number, which bounds this pages amount.
    /// If pages amount == 4GB size, then returns None, because such page number does not exist.
    pub fn to_page_number(&self) -> Option<Page<SIZE>> {
        self.unbound()
    }

    /// Returns corresponding amount of pages with another size `S`.
    pub fn to_pages_amount<const S: u32>(&self) -> PagesAmount<S> {
        let raw = if Self::SIZE > S {
            (Self::SIZE / S) * self.0
        } else {
            self.0 / (S / Self::SIZE)
        };
        PagesAmount(raw)
    }

    /// Returns amount in bytes.
    /// Can be also considered as offset of a page with corresponding number.
    /// In 32-bits address space it can be up to u32::MAX + 1,
    /// so we returns u64 to prevent overflow.
    pub fn offset(&self) -> u64 {
        self.0 as u64 * SIZE as u64
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
        match self.cmp(&Self::UPPER) {
            Ordering::Greater => {
                // This panic is impossible because of `PagesAmount` constructors implementation.
                let err_msg = format!(
                    "PagesAmount::unbound: PageBound must be always less or equal than UPPER. \
                    Page bound - {:?}, UPPER - {:?}",
                    self,
                    Self::UPPER
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
            Ordering::Equal => None,
            Ordering::Less => Some(Page(self.0)),
        }
    }
}

/// Try from u32 error for [PagesAmount].
#[derive(Debug, Clone, derive_more::Display)]
#[display("Tries to make pages amount from {_0}, which must be less or equal to {_1}")]
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

impl<const SIZE: u32> PartialEq<PagesAmount<SIZE>> for Page<SIZE> {
    fn eq(&self, other: &PagesAmount<SIZE>) -> bool {
        self.0 == other.0
    }
}

impl<const SIZE: u32> PartialOrd<PagesAmount<SIZE>> for Page<SIZE> {
    fn partial_cmp(&self, other: &PagesAmount<SIZE>) -> Option<Ordering> {
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
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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
        Self(offset / Self::SIZE)
    }

    /// Returns page zero byte offset.
    pub fn offset(&self) -> u32 {
        self.0 * Self::SIZE
    }

    /// Returns page last byte offset.
    pub fn end_offset(&self) -> u32 {
        self.0 * Self::SIZE + (Self::SIZE - 1)
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
#[display("Tries to make page from {_0}, which must be less or equal to {_1}")]
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
pub type WasmPage = Page<WASM_PAGE_SIZE>;
/// Page of gear page size - 16 kiB.
pub type GearPage = Page<GEAR_PAGE_SIZE>;
/// Pages amount for wasm page size - 64 kiB.
pub type WasmPagesAmount = PagesAmount<WASM_PAGE_SIZE>;
/// Pages amount for gear page size - 16 kiB.
pub type GearPagesAmount = PagesAmount<GEAR_PAGE_SIZE>;

impl From<u16> for WasmPagesAmount {
    fn from(value: u16) -> Self {
        const { assert!(WASM_PAGE_SIZE <= 0x10_000) };
        Self(value as u32)
    }
}

impl From<u16> for WasmPage {
    fn from(value: u16) -> Self {
        const { assert!(WASM_PAGE_SIZE <= 0x10_000) };
        Self(value as u32)
    }
}

impl From<u16> for GearPagesAmount {
    fn from(value: u16) -> Self {
        const { assert!(GEAR_PAGE_SIZE <= 0x10_000) };
        Self(value as u32)
    }
}

impl From<u16> for GearPage {
    fn from(value: u16) -> Self {
        const { assert!(GEAR_PAGE_SIZE <= 0x10_000) };
        Self(value as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    #[test]
    fn test_page_inc() {
        assert_eq!(WasmPage::from(10).inc(), WasmPagesAmount::from(11));
        assert_eq!(WasmPage::UPPER.inc(), WasmPagesAmount::UPPER);
    }

    #[test]
    fn test_page_from_offset() {
        assert_eq!(WasmPage::from_offset(WASM_PAGE_SIZE - 1), WasmPage::from(0));
        assert_eq!(WasmPage::from_offset(WASM_PAGE_SIZE), WasmPage::from(1));
        assert_eq!(WasmPage::from_offset(WASM_PAGE_SIZE + 1), WasmPage::from(1));
    }

    #[test]
    fn test_page_offset() {
        assert_eq!(WasmPage::from(80).offset(), 80 * WASM_PAGE_SIZE);
    }

    #[test]
    fn test_page_end_offset() {
        assert_eq!(
            WasmPage::from(80).end_offset(),
            80 * WASM_PAGE_SIZE + (WASM_PAGE_SIZE - 1)
        );
    }

    #[test]
    fn test_page_to_page() {
        assert_eq!(
            WasmPage::from(80).to_page::<GEAR_PAGE_SIZE>(),
            GearPage::from(80 * 4)
        );
    }

    #[test]
    fn test_page_to_iter() {
        assert_eq!(
            WasmPage::from(5).to_iter().collect::<Vec<GearPage>>(),
            vec![
                GearPage::from(20),
                GearPage::from(21),
                GearPage::from(22),
                GearPage::from(23)
            ]
        );
    }

    #[test]
    fn test_pages_amount_add() {
        let a = WasmPagesAmount::from(10);
        let b = WasmPagesAmount::from(20);
        assert_eq!(a.add(b), Some(WasmPagesAmount::from(30)));
        assert_eq!(a.add(WasmPagesAmount::UPPER), None);
    }

    #[test]
    fn test_pages_amount_to_page_number() {
        assert_eq!(
            WasmPagesAmount::from(10).to_page_number(),
            Some(WasmPage::from(10))
        );
        assert_eq!(WasmPagesAmount::UPPER.to_page_number(), None);
    }

    #[test]
    fn test_pages_amount_to_pages_amount() {
        assert_eq!(
            WasmPagesAmount::from(10).to_pages_amount::<GEAR_PAGE_SIZE>(),
            GearPagesAmount::from(40)
        );
        assert_eq!(
            GearPagesAmount::from(40).to_pages_amount::<WASM_PAGE_SIZE>(),
            WasmPagesAmount::from(10)
        );
    }

    #[test]
    fn test_pages_amount_offset() {
        assert_eq!(
            WasmPagesAmount::from(10).offset(),
            10 * WASM_PAGE_SIZE as u64
        );
        assert_eq!(WasmPagesAmount::UPPER.offset(), u32::MAX as u64 + 1);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use numerated::mock::{self, IntervalAction};
    use proptest::{
        prelude::{any, Arbitrary},
        proptest,
        strategy::{BoxedStrategy, Strategy},
        test_runner::Config as ProptestConfig,
    };

    impl<const S: u32> Arbitrary for Page<S> {
        type Parameters = ();
        type Strategy = BoxedStrategy<Page<S>>;

        fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
            (0..=Page::<S>::UPPER.0).prop_map(Page).boxed()
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn gear_page_numerated(x in any::<GearPage>(), y in any::<GearPage>()) {
            mock::test_numerated(x, y);
        }

        #[test]
        fn gear_page_interval(action in any::<IntervalAction<GearPage>>()) {
            mock::test_interval(action);
        }

        #[test]
        fn wasm_page_numerated(x in any::<WasmPage>(), y in any::<WasmPage>()) {
            mock::test_numerated(x, y);
        }

        #[test]
        fn wasm_page_interval(action in any::<IntervalAction<WasmPage>>()) {
            mock::test_interval(action);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn gear_page_tree((initial, actions) in mock::tree_actions::<GearPage>(0..128, 2..8)) {
            mock::test_tree(initial, actions);
        }

        #[test]
        fn wasm_page_tree((initial, actions) in mock::tree_actions::<WasmPage>(0..128, 10..20)) {
            mock::test_tree(initial, actions);
        }
    }
}
