// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    pub fn init(addr: gstd::ActorId) -> ! {
        let _ = gstd::msg::send_bytes(addr, b"PING", 0).unwrap();
        gstd::exec::wait_up_to(100)
    }

    pub fn handle_reply() -> ! {
        gstd::exec::exit(gstd::msg::source())
    }
}
