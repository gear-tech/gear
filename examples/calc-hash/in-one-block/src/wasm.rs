// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! This program runs a hashing computation in one execution.
//!
//! `Handle` method gets a [`Package`] in the payload, and repeatedly calculates a sha256 hash,
//! until the [`Package`] is finished. Once it is done, a [`reply()`] is sent, containing the result in
//! the payload.
//!
//! [`reply()`]: msg::reply

use crate::Package;
use gstd::msg;

#[no_mangle]
extern "C" fn handle() {
    let mut pkg = msg::load::<Package>().expect("Invalid initial data.");

    while !pkg.finished() {
        pkg.calc();
    }

    msg::reply(pkg.result(), 0).expect("Send reply failed.");
}
