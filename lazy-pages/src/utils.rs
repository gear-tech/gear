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

use crate::common::{Error, LazyPage, LazyPagesExecutionContext, PagePrefix};

/// Call `f` for all inclusive ranges from `indexes`.
/// For example: `indexes` = {1,2,3,5,6,7,9}, then `f` will be called
/// for 1..=3, 5..=7, 9..=9 consequently.
/// `indexes` must be sorted and uniq.
/// If `f` returns an Err then end execution without remain indexes handling.
pub fn with_inclusive_ranges<P: PageU32Size + Ord, E>(
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

pub fn handle_psg_case_one_page(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    page: LazyPage,
) -> Result<PagesIterInclusive<LazyPage>, Error> {
    // Accessed granularity page.
    let granularity_page: GranularityPage = page.to_page();
    // First gear page in accessed granularity page.
    let gear_page = granularity_page.to_page();

    let program_prefix = ctx
        .program_storage_prefix
        .as_ref()
        .ok_or(Error::ProgramPrefixIsNotSet)?;
    let prefix = PagePrefix::calc_once(program_prefix, gear_page);

    if !sp_io::storage::exists(&prefix) {
        Ok(granularity_page.to_pages_iter())
    } else {
        Ok(page.iter_once())
    }
}
