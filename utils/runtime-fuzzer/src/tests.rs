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

use super::gear_calls::GearCalls;
use crate::*;
use proptest::prelude::*;

const MAX_GEAR_CALLS_BYTES: usize = 30_000_000;
const MIN_GEAR_CALLS_BYTES: usize = 25_000_000;

#[test]
fn proptest_input_validity() {
    assert!(MIN_GEAR_CALLS_BYTES >= GearCalls::unstructured_size_hint());
    assert!(MIN_GEAR_CALLS_BYTES <= MAX_GEAR_CALLS_BYTES);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn test_fuzzer_reproduction(buf in prop::collection::vec(any::<u8>(), MIN_GEAR_CALLS_BYTES..MAX_GEAR_CALLS_BYTES)) {
        let ext1 = run_impl(&buf);
        let ext2 = run_impl(&buf);

        match (ext1, ext2) {
            (Ok(ext1), Ok(ext2)) => {
                assert!(ext1.eq(&ext2), "Both test-exts must be equal");
            }
            (ext1, ext2) => {
                ext1.expect("One or both of fuzzer runs failed");
                ext2.expect("One or both of fuzzer runs failed");
            }
        }
    }
}
