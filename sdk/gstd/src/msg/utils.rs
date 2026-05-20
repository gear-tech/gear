// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::prelude::ops::{Bound, RangeBounds};

pub(crate) fn decay_range(range: impl RangeBounds<usize>) -> (u32, u32) {
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
