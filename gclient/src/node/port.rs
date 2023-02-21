// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Picking random ports
use rand::Rng;
use std::{net::TcpListener, ops::Range};

/// localhost addr
const LOCALHOST: &str = "127.0.0.1";
const PORT_RANGE: Range<u16> = 15000..25000;

/// Pick a random port
pub fn pick() -> u16 {
    let mut rng = rand::thread_rng();

    loop {
        let port = rng.gen_range(PORT_RANGE);
        if TcpListener::bind(format!("{LOCALHOST}:{port}")).is_ok() {
            return port;
        }
    }
}
