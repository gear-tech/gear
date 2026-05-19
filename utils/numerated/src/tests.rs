// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
