// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use core::ops::Range;

/// Construct a range from an offset to a data length after the offset.
/// Returns None if the end of the range would exceed some maximum offset.
pub fn checked_range(offset: usize, len: usize, max: usize) -> Option<Range<usize>> {
    let end = offset.checked_add(len)?;
    (end <= max).then_some(offset..end)
}
