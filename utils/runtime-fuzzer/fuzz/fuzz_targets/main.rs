// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
