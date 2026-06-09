// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use runtime_fuzzer::{self, FuzzerInput};

fuzz_target!(|data: FuzzerInput<'_>| -> Corpus {
    gear_utils::init_default_logger();

    log::info!("Executing generated gear calls");

    match runtime_fuzzer::run(data) {
        Err(_) => Corpus::Reject,
        Ok(_) => Corpus::Keep,
    }
});
