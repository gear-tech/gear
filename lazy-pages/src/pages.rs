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

use numerated::{Bound, Interval, IntervalIterator, Numerated, OptionBound};
use std::{cmp::Ordering, marker::PhantomData, mem, num::NonZeroU32};

pub trait SizeNumber: Copy + Ord + Eq {
    const SIZE_NO: usize;
}

const WASM_SIZE_NO: usize = 0;
const GEAR_SIZE_NO: usize = 1;
pub const SIZES_AMOUNT: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct WasmSizeNo;

impl SizeNumber for WasmSizeNo {
    const SIZE_NO: usize = WASM_SIZE_NO;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct GearSizeNo;

impl SizeNumber for GearSizeNo {
    const SIZE_NO: usize = GEAR_SIZE_NO;
}

impl Copy for GearSizeNo {}
impl Copy for WasmSizeNo {}

/// Context where dynamic size pages store their sizes
pub trait SizeManager {
    /// Returns non-zero size of page.
    fn size_non_zero<S: SizeNumber>(&self) -> NonZeroU32;
}

#[cfg(test)]
impl SizeManager for u32 {
    fn size_non_zero<S: SizeNumber>(&self) -> NonZeroU32 {
        NonZeroU32::new(*self).expect("Size cannot be zero")
    }
}

pub(crate) type PagesAmount<S> = OptionBound<Page<S>>;

pub trait PagesAmountTrait<S: SizeNumber>: Bound<Page<S>> {
    fn upper<M: SizeManager>(ctx: &M) -> u32 {
        Page::<S>::max_value(ctx)
            .raw
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("Page size == 1 byte is restricted"))
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
        const _: () = assert!(mem::size_of::<usize>() > mem::size_of::<u32>());
        let raw = self.unbound().map(|p| p.raw).unwrap_or(Self::upper(ctx));
        (raw as usize)
            .checked_mul(Page::<S>::size(ctx) as usize)
            .unwrap_or_else(|| unreachable!("`self` page size has been changed - it's restricted"))
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
        u32::from(ctx.size_non_zero::<S>())
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
        self.raw
            .checked_mul(Self::size(ctx))
            .unwrap_or_else(|| unreachable!("`self` page size has been changed - it's restricted"))
    }

    /// Returns offset of end of page.
    pub fn end_offset<M: SizeManager>(&self, ctx: &M) -> u32 {
        let size = Self::size(ctx);
        self.raw
            .checked_mul(size)
            .and_then(|offset| offset.checked_add(size - 1))
            .unwrap_or_else(|| unreachable!("`self` page size has been changed - it's restricted"))
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
    type Bound = OptionBound<Self>;

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
mod tests {
    use super::*;
    use numerated::mock::test_numerated;
    use proptest::{
        arbitrary::any, proptest, strategy::Strategy, test_runner::Config as ProptestConfig,
    };
    use std::fmt::Debug;

    fn rand_page<S: SizeNumber + Debug>() -> impl Strategy<Value = Page<S>> {
        any::<u16>().prop_map(|raw| Page::from_raw(raw as u32))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn proptest_gear_page_numerated(p1 in rand_page::<GearSizeNo>(), p2 in rand_page::<GearSizeNo>()) {
            test_numerated(p1, p2);
        }

        #[test]
        fn proptest_wasm_page_numerated(p1 in rand_page::<WasmSizeNo>(), p2 in rand_page::<WasmSizeNo>()) {
            test_numerated(p1, p2);
        }
    }
}
