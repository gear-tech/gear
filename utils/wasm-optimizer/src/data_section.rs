// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

use pwasm_utils::parity_wasm::elements::{DataSection, DataSegment, Instruction};
use std::collections::BTreeMap;

/// Returns statistics of zero bytes between data segments.
/// Key - number of zero bytes between data segments.
/// Value - number of occurrences.
pub fn zero_bytes_gap_statistics(data_section: &DataSection) -> BTreeMap<usize, usize> {
    let mut statistics = BTreeMap::new();
    for pair in data_section.entries().windows(2) {
        let zero_bytes = match segments_zero_bytes_gap(&pair[0], &pair[1]) {
            Some(zero_bytes) => zero_bytes,
            // skip `passive` data segments
            None => continue,
        };
        *statistics.entry(zero_bytes).or_insert(0) += 1;
    }

    statistics
}

/// Returns the number of zero bytes between two data segments.
/// Formula: `end_segment_offset - start_segment_offset - start_segment_size`
pub fn segments_zero_bytes_gap(
    start_segment: &DataSegment,
    end_segment: &DataSegment,
) -> Option<usize> {
    let offset = |segment: &DataSegment| match segment.offset().clone()?.code().first()? {
        Instruction::I32Const(value) => Some(*value),
        _ => None,
    };

    let start_segment_offset: usize = offset(start_segment)?.try_into().unwrap();
    let end_segment_offset: usize = offset(end_segment)?.try_into().unwrap();

    Some(end_segment_offset - start_segment_offset - start_segment.value().len())
}
