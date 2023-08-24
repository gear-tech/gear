// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::arbitrary_call::MIN_GEAR_CALLS_BYTES;
use crate::*;
use arbitrary::{Arbitrary, Unstructured};
use proptest::prelude::*;

const MAX_GEAR_CALLS_BYTES: usize = 30_000_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn test_fuzzer_reproduction(buf in prop::collection::vec(any::<u8>(), MIN_GEAR_CALLS_BYTES..MAX_GEAR_CALLS_BYTES)) {
        let mut u1 = Unstructured::new(&buf);
        let mut u2 = Unstructured::new(&buf);

        let calls1 = GearCalls::arbitrary(&mut u1).expect("failed gear calls creation");
        let calls2 = GearCalls::arbitrary(&mut u2).expect("failed gear calls creation");

        assert_eq!(calls1, calls2);

        let ext1 = run_impl(calls1);
        let ext2 = run_impl(calls2);

        assert!(ext1.eq(&ext2), "Both test-exts must be equal");
    }
}
