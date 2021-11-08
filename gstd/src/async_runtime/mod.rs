// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

pub mod futures;
pub mod signals;

pub use crate::async_runtime::futures::*;
pub use crate::async_runtime::signals::*;

#[allow(clippy::missing_safety_doc)]
#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    let original_message_id = crate::msg::reply_to();
    self::signals::signals_static()
        .record_reply(original_message_id, crate::msg::load_bytes());
}
