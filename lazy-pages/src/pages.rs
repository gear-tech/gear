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

use gear_core::pages::{Bound, BoundValue, Numerated};
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
    /// Returns size of page.
    fn size<S: SizeNumber>(&self) -> u32 {
        self.size_non_zero::<S>().into()
    }
    fn upper_page_number<S: SizeNumber>(&self) -> u32 {
        u32::MAX
            .checked_div(self.size::<S>())
            .unwrap_or_else(|| unreachable!("Page size == 0 bytes - restricted!!!"))
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("Page size == 1 bytes - restricted!!!"))
    }
}

impl SizeManager for u32 {
    fn size_non_zero<S: SizeNumber>(&self) -> NonZeroU32 {
        NonZeroU32::new(*self).unwrap_or_else(|| unreachable!("Size cannot be zero"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PagesAmount<S: SizeNumber> {
    raw: u32,
    is_upper: bool,
    _phantom: PhantomData<S>,
}

impl<S: SizeNumber> PagesAmount<S> {
    fn from_raw(raw: u32, is_upper: bool) -> Self {
        Self {
            raw,
            is_upper,
            _phantom: PhantomData,
        }
    }

    pub fn new<M: SizeManager>(ctx: &M, raw: u32) -> Option<Self> {
        match raw.cmp(&ctx.upper_page_number::<S>()) {
            Ordering::Less => Some(Self::from_raw(raw, false)),
            Ordering::Equal => Some(Self::from_raw(raw, true)),
            Ordering::Greater => None,
        }
    }

    pub fn from_option<M: SizeManager>(ctx: &M, p: Option<Page<S>>) -> Self {
        p.map_or_else(
            || Self::from_raw(ctx.upper_page_number::<S>(), true),
            |p| Self::from_raw(p.raw(), false),
        )
    }

    pub fn offset<M: SizeManager>(&self, ctx: &M) -> usize {
        static_assertions::const_assert!(mem::size_of::<usize>() > mem::size_of::<u32>());
        self.raw as usize * Page::<S>::size(ctx) as usize
    }
}

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

    pub fn raw(&self) -> u32 {
        self.raw
    }

    pub fn size<M: SizeManager>(ctx: &M) -> u32 {
        ctx.size::<S>()
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
        self.raw * Self::size(ctx)
    }

    /// Returns offset of end of page.
    pub fn end_offset<M: SizeManager>(&self, ctx: &M) -> u32 {
        let size = Self::size(ctx);
        self.raw * size + (size - 1)
    }

    /// Creates page from offset.
    pub fn from_offset<M: SizeManager>(ctx: &M, offset: u32) -> Self {
        Self::from_raw(offset / Self::size(ctx))
    }

    pub fn to_page<M: SizeManager, S1: SizeNumber>(self, ctx: &M) -> Page<S1> {
        Page::<S1>::from_offset(ctx, self.offset(ctx))
    }
}

impl<S: SizeNumber> From<Page<S>> for PagesAmount<S> {
    fn from(page: Page<S>) -> Self {
        Self {
            raw: page.raw,
            is_upper: false,
            _phantom: PhantomData,
        }
    }
}

impl<S: SizeNumber> From<PagesAmount<S>> for Option<Page<S>> {
    fn from(page: PagesAmount<S>) -> Self {
        if page.is_upper {
            None
        } else {
            Some(Page::from_raw(page.raw))
        }
    }
}

impl<S: SizeNumber> Bound<Page<S>> for PagesAmount<S> {
    fn unbound(self) -> BoundValue<Page<S>> {
        if self.is_upper {
            BoundValue::Upper(Page::from_raw(
                self.raw
                    .checked_sub(1)
                    .unwrap_or_else(|| unreachable!("Upper page number is 0")),
            ))
        } else {
            BoundValue::Value(Page::from_raw(self.raw))
        }
    }
}

impl<S: SizeNumber> Numerated for Page<S> {
    type N = u32;
    type B = PagesAmount<S>;

    fn add_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self> {
        self.raw.checked_add(num).and_then(|r| {
            r.enclosed_by(&self.raw, &other.raw)
                .then_some(Self::from_raw(r))
        })
    }

    fn sub_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self> {
        self.raw.checked_sub(num).and_then(|r| {
            r.enclosed_by(&self.raw, &other.raw)
                .then_some(Self::from_raw(r))
        })
    }

    fn distance(self, other: Self) -> Option<Self::N> {
        self.raw.checked_sub(other.raw)
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
