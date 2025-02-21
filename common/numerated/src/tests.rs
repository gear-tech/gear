// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

//! Property testing for Numerated, Interval and IntervalsTree.

use crate::mock::{self, IntervalAction};
use proptest::{arbitrary::any, proptest, test_runner::Config as ProptestConfig};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn proptest_numerated_i16(x in any::<i16>(), y in any::<i16>()) {
        mock::test_numerated(x, y);
    }

    #[test]
    fn proptest_interval_i16(action in any::<IntervalAction::<i16>>()) {
        mock::test_interval(action);
    }

    #[test]
    fn proptest_numerated_u16(x in any::<u16>(), y in any::<u16>()) {
        mock::test_numerated(x, y);
    }

    #[test]
    fn proptest_interval_u16(action in any::<IntervalAction::<u16>>()) {
        mock::test_interval(action);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn proptest_tree_i16((initial, actions) in mock::tree_actions::<i16>(0..1000, 10..20)) {
        mock::test_tree(initial, actions);
    }

    #[test]
    fn proptest_tree_u16((initial, actions) in mock::tree_actions::<i16>(0..1000, 10..20)) {
        mock::test_tree(initial, actions);
    }
}
