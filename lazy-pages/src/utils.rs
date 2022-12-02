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

use std::{iter::Step, ops::RangeInclusive};

#[derive(Debug, Clone, Copy, derive_more::Display)]
pub enum WithInclusiveRangesError {
    #[display(fmt = "forward_checked overflow")]
    Overflow,
    #[display(fmt = "Indexes must be sorted by ascending and be uniq")]
    IndexesAreNotSorted,
}

/// Call `f` for all inclusive ranges from `indexes`.
/// For example: `indexes` = {1,2,3,5,6,7,9}, then `f` will be called
/// for 1..=3, 5..=7, 9..=9 consequently.
/// `indexes` must be sorted and uniq.
/// If `f` returns an Err then end execution without remain indexes handling.
pub fn with_inclusive_ranges<T: Sized + Copy + Eq + Step, E>(
    mut indexes: impl Iterator<Item = T>,
    mut f: impl FnMut(RangeInclusive<T>) -> Result<(), E>,
) -> Result<Result<(), E>, WithInclusiveRangesError> {
    let mut start = if let Some(start) = indexes.next() {
        start
    } else {
        return Ok(Ok(()));
    };
    let mut end = start;
    for idx in indexes {
        if end >= idx {
            return Err(WithInclusiveRangesError::IndexesAreNotSorted);
        }
        if Step::forward_checked(end, 1).ok_or(WithInclusiveRangesError::Overflow)? != idx {
            if let Err(err) = f(start..=end) {
                return Ok(Err(err));
            }
            start = idx;
        }
        end = idx;
    }

    Ok(f(start..=end))
}
