// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::*;
use proptest::prelude::*;

const MIN_GEAR_CALLS_BYTES: usize = 350_000;
const MAX_GEAR_CALLS_BYTES: usize = 450_000;

#[test]
fn proptest_input_validity() {
    let min_unstructured_input_size =
        GearCallsGenerator::random_data_requirement() + BalanceManager::random_data_requirement();

    assert!(MIN_GEAR_CALLS_BYTES >= min_unstructured_input_size);
    const _: () = assert!(MIN_GEAR_CALLS_BYTES <= MAX_GEAR_CALLS_BYTES);
}

#[test]
fn test_corpus_aa91f1d5873e3fa7045ceeef9e26448e71f82482() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/aa91f1d5873e3fa7045ceeef9e26448e71f82482");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// This is a crashing input before c85f4563ce35d822958a23a92d85f798252c8466 commit to master.
#[test]
fn test_corpus_c6e2a597aebabecc9bbb11eefdaa4dd8a6770188() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/c6e2a597aebabecc9bbb11eefdaa4dd8a6770188");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// This is a crashing input before https://github.com/gear-tech/gear/pull/3678 was merged to master.
// The 25b29b826 commit to master was adapted by removing fixes from #3678 to test the bug.
#[test]
fn test_corpus_3a8de0dbd12a8a5e6e77bf5a81f2c511816a0ec2() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/3a8de0dbd12a8a5e6e77bf5a81f2c511816a0ec2");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// This is a crashing input before https://github.com/gear-tech/gear/pull/3152 was merged to master.
// The 25b29b826 commit to master was adapted by removing fixes from #3152 to test the bug.
#[test]
fn test_corpus_03d70a0b413038867c102a2b52f56b637ca6b277() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/03d70a0b413038867c102a2b52f56b637ca6b277");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// This is a crashing input before https://github.com/gear-tech/gear/pull/2745 was merged to master.
// The 25b29b826 commit to master was adapted by removing fixes from #2745 to test the bug.
#[test]
fn test_corpus_888e61e90b2d419b40d362af7036387364eef9bf() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/888e61e90b2d419b40d362af7036387364eef9bf");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// This is a crashing input before https://github.com/gear-tech/gear/pull/976 was merged to master.
// The 25b29b826 commit to master was adapted by removing fixes from #976 to test the bug.
#[test]
fn test_corpus_bfcd26c09ccc57e35aaabc0937b0711520bddf75() {
    gear_utils::init_default_logger();

    let input = include_bytes!("../fuzz_corpus/bfcd26c09ccc57e35aaabc0937b0711520bddf75");
    assert!(run_impl(FuzzerInput::new(input)).is_ok());
}

// TODO #3900 implement test for the input b09f6282c0e906c06168e7fc22fb5417c3f6fafb-pr-3496-gas5k-delay10k.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn test_fuzzer_reproduction(buf in prop::collection::vec(any::<u8>(), MIN_GEAR_CALLS_BYTES..MAX_GEAR_CALLS_BYTES)) {
        let ext1 = run_impl(FuzzerInput::new(&buf));
        let ext2 = run_impl(FuzzerInput::new(&buf));

        match (ext1, ext2) {
            (Ok(mut ext1), Ok(mut ext2)) => {
                assert!(ext1.eq(&mut ext2), "Both test-exts must be equal");
            }
            (ext1, ext2) => {
                ext1.expect("One or both of fuzzer runs failed");
                ext2.expect("One or both of fuzzer runs failed");
            }
        }
    }
}
