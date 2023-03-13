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

//! Utils.

use std::{cell::RefMut, collections::BTreeSet};

use gear_core::memory::{GranularityPage, PageU32Size, PagesIterInclusive};

use crate::common::{Error, LazyPage, LazyPagesExecutionContext};

/// Call `f` for all inclusive ranges from `indexes`.
/// For example: `indexes` = {1,2,3,5,6,7,9}, then `f` will be called
/// for 1..=3, 5..=7, 9..=9 consequently.
/// `indexes` must be sorted and uniq.
/// If `f` returns an Err then end execution without remain indexes handling.
pub(crate) fn with_inclusive_ranges<P: PageU32Size + Ord, E>(
    pages: &BTreeSet<P>,
    mut f: impl FnMut(PagesIterInclusive<P>) -> Result<(), E>,
) -> Result<(), E> {
    let mut pages_iter = pages.iter();
    let mut start = match pages_iter.next() {
        Some(&start) => start,
        None => return Ok(()),
    };
    let mut end = start;
    for &page in pages_iter {
        let after_end = end.inc().unwrap_or_else(|err| {
            unreachable!(
                "`pages` is btree set, so `end` must be smaller then page, but get: {}",
                err
            )
        });
        if after_end != page {
            let iter = start.iter_end_inclusive(end).unwrap_or_else(|err| {
                unreachable!(
                    "`pages is btree set, so `end` must be bigger or equal than start, but get: {}",
                    err
                )
            });
            f(iter)?;
            start = page;
        }
        end = page;
    }

    let iter = start.iter_end_inclusive(end).unwrap_or_else(|err| {
        unreachable!(
            "`pages is btree set, so `end` must be bigger or equal than start, but get: {}",
            err
        )
    });
    f(iter)
}

pub(crate) fn handle_psg_case_one_page(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    page: LazyPage,
) -> Result<PagesIterInclusive<LazyPage>, Error> {
    if ctx.page_has_data_in_storage(page.to_page())? {
        // if at least one gear page has data in storage, then all pages from corresponding
        // `GranularityPage` have data in storage, therefor all gear pages from `page`
        // have data in storage. So, this is not psg case and we can handle only one `LazyPage`.
        Ok(page.iter_once())
    } else {
        //
        Ok(page.to_page::<GranularityPage>().to_pages_iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_inclusive_range() {
        use gear_core::memory::GearPage;

        let test = |pages: &[u16]| {
            let mut inclusive_ranges: Vec<Vec<u32>> = Vec::new();
            let slice_to_ranges = |iter: PagesIterInclusive<GearPage>| -> Result<(), ()> {
                inclusive_ranges.push(iter.map(|p| p.raw()).collect());
                Ok(())
            };

            with_inclusive_ranges(
                &pages.iter().copied().map(GearPage::from).collect(),
                slice_to_ranges,
            )
            .unwrap();
            inclusive_ranges
        };

        let mut res = test([1, 2, 5, 6, 7, 11, 19].as_slice());
        assert_eq!(res.pop().unwrap(), vec![19]);
        assert_eq!(res.pop().unwrap(), vec![11]);
        assert_eq!(res.pop().unwrap(), vec![5, 6, 7]);
        assert_eq!(res.pop().unwrap(), vec![1, 2]);

        let mut res = test([5, 6, 7, 8, 9, 10, 11].as_slice());
        assert_eq!(res.pop().unwrap(), vec![5, 6, 7, 8, 9, 10, 11]);

        let mut res = test([5, 6, 7, 8, 9, 10, 11, 90].as_slice());
        assert_eq!(res.pop().unwrap(), vec![90]);
        assert_eq!(res.pop().unwrap(), vec![5, 6, 7, 8, 9, 10, 11]);
    }
}
