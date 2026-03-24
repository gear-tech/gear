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

use gear_core::buffer::Payload;
use gear_core_errors::{ReplyCode, SignalCode};

#[test]
fn check_message_codes_string_len() {
    for reply_code in enum_iterator::all::<ReplyCode>() {
        let _: Payload = reply_code.to_string().into_bytes().try_into().unwrap();
    }

    for signal_code in enum_iterator::all::<SignalCode>() {
        let _: Payload = signal_code.to_string().into_bytes().try_into().unwrap();
    }
}
