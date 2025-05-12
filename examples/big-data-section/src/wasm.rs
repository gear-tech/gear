// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::data_access::DataAccess;
use gstd::msg;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    let value = DataAccess::from_payload(&payload)
        .expect("Failed to decode incoming payload")
        .constant();

    msg::reply_bytes(value.to_be_bytes(), 0).expect("Failed to send reply");
}
