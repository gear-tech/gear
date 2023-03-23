// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Memory pages, which can change their sizes during node execution,
//! for example if runtime decided to change gear page size.

// TODO: join with page realisation in gear-core #2440

use std::num::NonZeroU32;

/// Page number trait - page, which can return it number as u32.
pub trait PageNumber: Into<u32> + Sized + Copy + Clone {
    unsafe fn from_raw(raw: u32) -> Self;

    fn raw(&self) -> u32 {
        Into::<u32>::into(*self)
    }

    fn checked_sub(&self, other: Self) -> Option<Self> {
        self.raw()
            .checked_sub(other.raw())
            .map(|p| unsafe { Self::from_raw(p) })
    }

    /// Returns iterator `self`..=`end`.
    fn iter_end_inclusive(&self, end: Self) -> Option<PagesIterInclusive<Self>> {
        (end.raw() >= self.raw()).then_some(PagesIterInclusive {
            page: Some(*self),
            end,
        })
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

    fn next(&mut self) -> Option<P> {
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

pub(crate) enum PageSizeNo {
    WasmSizeNo = 0,
    GearSizeNo = 1,
    Amount = 2,
}

/// Context where dynamic size pages store their sizes
pub(crate) trait SizeManager {
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32;
    fn size<P: PageDynSize>(&self) -> u32 {
        self.size_non_zero::<P>().into()
    }
}

pub(crate) trait PageDynSize: PageNumber {
    const SIZE_NO: usize;
    fn size<S: SizeManager>(ctx: &S) -> u32 {
        ctx.size::<Self>()
    }
    fn new<S: SizeManager>(raw: u32, ctx: &S) -> Option<Self> {
        let page_size = Self::size(ctx);
        let page_begin = raw.checked_mul(page_size)?;

        // Check that the last page byte has index less or equal then u32::MAX
        let last_byte_offset = page_size - 1;
        page_begin.checked_add(last_byte_offset)?;

        Some(unsafe { Self::from_raw(raw) })
    }
    fn offset<S: SizeManager>(&self, ctx: &S) -> u32 {
        self.raw() * Self::size(ctx)
    }
    fn end_offset<S: SizeManager>(&self, ctx: &S) -> u32 {
        let size = Self::size(ctx);
        self.raw() * size + (size - 1)
    }
    fn from_offset<S: SizeManager>(ctx: &S, offset: u32) -> Self {
        unsafe { Self::from_raw(offset / Self::size(ctx)) }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
pub struct WasmPageNumber(u32);

impl PageNumber for WasmPageNumber {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl PageDynSize for WasmPageNumber {
    const SIZE_NO: usize = PageSizeNo::WasmSizeNo as usize;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
pub struct GearPageNumber(u32);

impl PageNumber for GearPageNumber {
    unsafe fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl PageDynSize for GearPageNumber {
    const SIZE_NO: usize = PageSizeNo::GearSizeNo as usize;
}

#[cfg(test)]
impl SizeManager for u32 {
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32 {
        NonZeroU32::new(*self).expect("Size cannot be zero")
    }
}
