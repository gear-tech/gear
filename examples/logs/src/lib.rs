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

#![cfg_attr(not(test), no_std)]

const BYTES: &[u8] = b"bytes: 42";
const STRING: &str = "string: 42";

#[no_mangle]
extern "C" fn init() {
    gstd::msg::log(BYTES).expect("Failed to log raw bytes");
    gstd::msg::log_str(STRING).expect("Failed to log str");
    gstd::log!("string: {}", 42).expect("Failed to log raw bytes");
}

#[no_mangle]
extern "C" fn handle() {}

#[cfg(test)]
mod tests {
    use crate::{BYTES, STRING};
    use gtest::{Program, System};

    #[test]
    fn test_logs() {
        gtest::ensure_gbuild(false);

        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);
        let result = program.send(42, *b"");
        let logs = result
            .log()
            .iter()
            .filter_map(|l| {
                if l.destination().is_zero() {
                    return Some(l.payload());
                }
                None
            })
            .collect::<Vec<_>>();

        assert_eq!(logs, vec![BYTES, STRING.as_bytes(), STRING.as_bytes()])
    }
}
