// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use junit_common::TestSuites;
use std::{collections::BTreeMap, str::FromStr};

pub fn build_tree<Filter>(
    filter: Filter,
    test_suites: TestSuites,
) -> BTreeMap<String, BTreeMap<String, f64>>
where
    Filter: Fn(&str) -> bool,
{
    test_suites
        .testsuite
        .into_iter()
        .filter_map(|test_suite| {
            if !filter(&test_suite.name) {
                return None;
            }

            let pallet_suite = test_suite
                .testcase
                .into_iter()
                .map(|test_case| (test_case.name, f64::from_str(&test_case.time).unwrap()))
                .collect::<BTreeMap<_, _>>();

            Some((test_suite.name, pallet_suite))
        })
        .collect::<BTreeMap<_, _>>()
}
