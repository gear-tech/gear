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

//! Module for pages which size can be different for different runtime versions.

use numerated::{Bound, Numerated, OptionBound, interval::Interval, iterators::IntervalIterator};
use std::{cmp::Ordering, marker::PhantomData, num::NonZero};

/// Size number for dyn-size pages.
pub trait SizeNumber: Copy + Ord + Eq {
    const SIZE_NO: usize;
}

const WASM_SIZE_NO: usize = 0;
const GEAR_SIZE_NO: usize = 1;

/// Amount of different page sizes.
///
/// NOTE: Must be in connect with current runtime.
/// If runtime wanna to reduce or increase amount of pages with different size,
/// then we must add here support for old runtimes that used 2 sizes,
/// and add support for new runtimes that uses some other amount of sizes.
pub(crate) const SIZES_AMOUNT: usize = 2;

/// Size number for wasm pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct WasmSizeNo;

impl SizeNumber for WasmSizeNo {
    const SIZE_NO: usize = WASM_SIZE_NO;
}

/// Size number for gear pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct GearSizeNo;

impl SizeNumber for GearSizeNo {
    const SIZE_NO: usize = GEAR_SIZE_NO;
}

/// Context where dynamic size pages store their sizes
pub trait SizeManager {
    /// Returns non-zero size of page.
    fn size_non_zero<S: SizeNumber>(&self) -> NonZero<u32>;
}

#[cfg(test)]
impl SizeManager for u32 {
    fn size_non_zero<S: SizeNumber>(&self) -> NonZero<u32> {
        NonZero::<u32>::new(*self).expect("Size cannot be zero")
    }
}

pub type PagesAmount<S> = OptionBound<Page<S>>;

pub trait PagesAmountTrait<S: SizeNumber>: Bound<Page<S>> {
    fn upper<M: SizeManager>(ctx: &M) -> u32 {
        Page::<S>::max_value(ctx)
            .raw
            .checked_add(1)
            .unwrap_or_else(|| {
                let err_msg = format!(
                    "Bound::upper: Page size == 1 byte is restricted. \
                    Page max value - {}",
                    Page::<S>::max_value(ctx).raw
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            })
    }
    fn new<M: SizeManager>(ctx: &M, raw: u32) -> Option<Self> {
        let page = match raw.cmp(&Self::upper(ctx)) {
            Ordering::Less => Some(Page::from_raw(raw)),
            Ordering::Equal => None,
            Ordering::Greater => return None,
        };
        Some(page.into())
    }
    fn offset<M: SizeManager>(&self, ctx: &M) -> usize {
        const { assert!(size_of::<usize>() > size_of::<u32>()) };
        let raw = self.unbound().map(|p| p.raw).unwrap_or(Self::upper(ctx));
        (raw as usize)
            .checked_mul(Page::<S>::size(ctx) as usize)
            .unwrap_or_else(|| {
                let err_msg = format!(
                    "Bound::offset: changing page size during program execution is restricted. \
                    Page number - {raw}, page size - {}",
                    Page::<S>::size(ctx)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            })
    }
    fn convert<M: SizeManager, S1: SizeNumber>(self, ctx: &M) -> PagesAmount<S1> {
        self.unbound().map(|p| p.to_page::<_, S1>(ctx)).into()
    }
}

impl<S: SizeNumber> PagesAmountTrait<S> for PagesAmount<S> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, derive_more::Into)]
pub struct Page<S: SizeNumber> {
    raw: u32,
    _phantom: PhantomData<S>,
}

impl<S: SizeNumber> Page<S> {
    fn from_raw(raw: u32) -> Self {
        Page {
            raw,
            _phantom: PhantomData,
        }
    }

    /// Returns raw page number.
    pub fn raw(&self) -> u32 {
        self.raw
    }

    /// Page size. Guaranteed to be > 0.
    pub fn size<M: SizeManager>(ctx: &M) -> u32 {
        ctx.size_non_zero::<S>().into()
    }

    /// Returns max possible page number for 32-bits address space.
    pub fn max_value<M: SizeManager>(ctx: &M) -> Self {
        Self::from_raw(u32::MAX / Self::size(ctx))
    }

    /// Creates page from raw number with specific context and checks that page number is valid.
    /// Returns None if page number is invalid.
    pub fn new<M: SizeManager>(ctx: &M, raw: u32) -> Option<Self> {
        let page_size = Self::size(ctx);
        let page_begin = raw.checked_mul(page_size)?;

        // Check that the last page byte has index less or equal then u32::MAX
        let last_byte_offset = page_size - 1;
        page_begin.checked_add(last_byte_offset)?;

        Some(Page::from_raw(raw))
    }

    /// Returns offset of page.
    pub fn offset<M: SizeManager>(&self, ctx: &M) -> u32 {
        self.raw.checked_mul(Self::size(ctx)).unwrap_or_else(|| {
            let err_msg = format!(
                "Bound::offset: `self` page size has been changed - it's restricted. \
                    Page number - {}, page size - {}",
                self.raw,
                Self::size(ctx)
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        })
    }

    /// Returns offset of end of page.
    pub fn end_offset<M: SizeManager>(&self, ctx: &M) -> u32 {
        let size = Self::size(ctx);
        self.raw
            .checked_mul(size)
            .and_then(|offset| offset.checked_add(size - 1))
            .unwrap_or_else(|| {
                let err_msg = format!(
                    "Bound::end_offset: `self` page size has been changed - it's restricted. \
                    Page number - {}, page size - {size}",
                    self.raw,
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            })
    }

    /// Creates page from offset.
    pub fn from_offset<M: SizeManager>(ctx: &M, offset: u32) -> Self {
        Self::from_raw(offset / Self::size(ctx))
    }

    /// Returns page with other size `S1`, which contains `self` start byte.
    pub fn to_page<M: SizeManager, S1: SizeNumber>(self, ctx: &M) -> Page<S1> {
        Page::<S1>::from_offset(ctx, self.offset(ctx))
    }

    /// Returns [IntervalIterator] created from `self..end`, if `end >= self`.
    pub fn to_end_interval<M: SizeManager, I: Into<PagesAmount<S>>>(
        self,
        ctx: &M,
        end: I,
    ) -> Option<IntervalIterator<Page<S>>> {
        match Into::<PagesAmount<S>>::into(end).unbound() {
            Some(end) => Interval::try_from_range(self..end).map(Into::into).ok(),
            None => (self..=Self::max_value(ctx)).try_into().ok(),
        }
    }
}

impl<S: SizeNumber> Numerated for Page<S> {
    type Distance = u32;
    type Bound = PagesAmount<S>;

    fn add_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
        self.raw.checked_add(num).and_then(|r| {
            r.enclosed_by(&self.raw, &other.raw)
                .then_some(Self::from_raw(r))
        })
    }

    fn sub_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
        self.raw.checked_sub(num).and_then(|r| {
            r.enclosed_by(&self.raw, &other.raw)
                .then_some(Self::from_raw(r))
        })
    }

    fn distance(self, other: Self) -> Self::Distance {
        self.raw.abs_diff(other.raw)
    }
}

pub type GearPage = Page<GearSizeNo>;
pub type WasmPage = Page<WasmSizeNo>;
pub type WasmPagesAmount = PagesAmount<WasmSizeNo>;
pub type GearPagesAmount = PagesAmount<GearSizeNo>;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fmt::Debug;

    #[derive(Debug, Clone, Copy)]
    pub struct PageSizeManager(pub [u32; 2]);

    impl SizeManager for PageSizeManager {
        fn size_non_zero<S: SizeNumber>(&self) -> NonZero<u32> {
            NonZero::<u32>::new(self.0[S::SIZE_NO]).expect("Size cannot be zero")
        }
    }

    impl Default for PageSizeManager {
        fn default() -> Self {
            PageSizeManager([0x10000, 0x4000])
        }
    }

    #[test]
    fn raw() {
        let page = GearPage::from_raw(0x100);
        assert_eq!(page.raw(), 0x100);
    }

    #[test]
    fn size() {
        let ctx = PageSizeManager::default();
        assert_eq!(GearPage::size(&ctx), 0x4000);
        assert_eq!(WasmPage::size(&ctx), 0x10000);
    }

    #[test]
    fn max_value() {
        let ctx = PageSizeManager::default();
        assert_eq!(GearPage::max_value(&ctx).raw(), 0x3FFFF);
        assert_eq!(WasmPage::max_value(&ctx).raw(), 0xFFFF);
    }

    #[test]
    fn new() {
        let ctx = PageSizeManager::default();
        assert_eq!(
            GearPage::new(&ctx, 0x3FFFF),
            Some(GearPage::from_raw(0x3FFFF))
        );
        assert_eq!(GearPage::new(&ctx, 0x40000), None);
        assert_eq!(
            WasmPage::new(&ctx, 0xFFFF),
            Some(WasmPage::from_raw(0xFFFF))
        );
        assert_eq!(WasmPage::new(&ctx, 0x10000), None);
    }

    #[test]
    fn offset() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_raw(0x100);
        assert_eq!(page.offset(&ctx), 0x100 * ctx.0[GearSizeNo::SIZE_NO]);
    }

    #[test]
    fn end_offset() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_raw(0x100);
        assert_eq!(
            page.end_offset(&ctx),
            0x100 * ctx.0[GearSizeNo::SIZE_NO] + (ctx.0[GearSizeNo::SIZE_NO] - 1)
        );
    }

    #[test]
    fn from_offset() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_offset(&ctx, 0x100 * ctx.0[GearSizeNo::SIZE_NO]);
        assert_eq!(page.raw(), 0x100);
    }

    #[test]
    fn to_page() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_raw(0x400);
        let page1 = page.to_page::<_, WasmSizeNo>(&ctx);
        assert_eq!(page1.raw(), 0x100);
    }

    #[test]
    fn to_end_interval() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_raw(0x100);
        let interval: Vec<_> = page
            .to_end_interval(&ctx, GearPage::from_raw(0x200))
            .unwrap()
            .collect();
        assert_eq!(
            interval,
            (0x100..0x200).map(GearPage::from_raw).collect::<Vec<_>>()
        );
    }

    #[test]
    fn pages_amount_upper() {
        let ctx = PageSizeManager::default();
        assert_eq!(GearPagesAmount::upper(&ctx), 0x40000);
        assert_eq!(WasmPagesAmount::upper(&ctx), 0x10000);
    }

    #[test]
    fn pages_amount_new() {
        let ctx = PageSizeManager::default();
        assert_eq!(
            GearPagesAmount::new(&ctx, 0x3FFFF),
            Some(GearPage::from_raw(0x3FFFF).into())
        );
        assert_eq!(GearPagesAmount::new(&ctx, 0x40000), Some(None.into()));
        assert_eq!(GearPagesAmount::new(&ctx, 0x40001), None);
        assert_eq!(
            WasmPagesAmount::new(&ctx, 0xFFFF),
            Some(WasmPage::from_raw(0xFFFF).into())
        );
        assert_eq!(WasmPagesAmount::new(&ctx, 0x10000), Some(None.into()));
        assert_eq!(WasmPagesAmount::new(&ctx, 0x10001), None);
    }

    #[test]
    fn pages_amount_offset() {
        let ctx = PageSizeManager::default();
        let page = GearPage::from_raw(0x100);
        assert_eq!(
            GearPagesAmount::offset(&page.into(), &ctx),
            0x100 * ctx.0[GearSizeNo::SIZE_NO] as usize
        );
    }

    #[test]
    fn pages_amount_convert() {
        let ctx = PageSizeManager::default();
        let page1: GearPagesAmount = GearPage::from_raw(0x400).into();
        let page2: WasmPagesAmount = page1.convert(&ctx);
        assert_eq!(page2, Some(WasmPage::from_raw(0x100)));
    }
}

#[cfg(test)]
mod property_tests {
    use super::{tests::PageSizeManager, *};
    use numerated::mock;
    use proptest::{
        prelude::{Arbitrary, any},
        proptest,
        strategy::{BoxedStrategy, Strategy},
        test_runner::Config as ProptestConfig,
    };
    use std::fmt::Debug;

    impl<S: SizeNumber + Debug + 'static> Arbitrary for Page<S> {
        type Parameters = PageSizeManager;
        type Strategy = BoxedStrategy<Page<S>>;

        fn arbitrary_with(ctx: Self::Parameters) -> Self::Strategy {
            (0..Page::<S>::max_value(&ctx).raw)
                .prop_map(Page::from_raw)
                .boxed()
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn gear_page_numerated(x in any::<GearPage>(), y in any::<GearPage>()) {
            mock::test_numerated(x, y);
        }

        #[test]
        fn wasm_page_numerated(x in any::<WasmPage>(), y in any::<WasmPage>()) {
            mock::test_numerated(x, y);
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
