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

use crate::prelude::ops::{Bound, RangeBounds};

pub(crate) fn decay_range<Range: RangeBounds<usize>>(range: Range) -> (u32, u32) {
    use Bound::*;

    let offset = match range.start_bound() {
        Unbounded => 0,
        Included(s) => *s,
        Excluded(s) => *s + 1,
    };

    let len = match range.end_bound() {
        Unbounded => u32::MAX,
        Included(e) if *e >= offset => (*e - offset + 1) as u32,
        Excluded(e) if *e >= offset => (*e - offset) as u32,
        _ => 0,
    };

    (offset as u32, len)
}
