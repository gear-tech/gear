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

use super::*;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

pub fn parse_wat(source: &str) -> Vec<u8> {
    let code = wat::parse_str(source).expect("failed to parse module");
    wasmparser::validate(&code).expect("failed to validate module");
    code
}

pub fn h256_code_hash(code: &[u8]) -> H256 {
    CodeId::generate(code).into_origin()
}
