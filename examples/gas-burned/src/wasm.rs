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

//! This smart contract is used to test burning gas. The `init` and `handle` functions both create
//! a [`Vec`] of lengths [`SHORT`] and [`LONG`] respectively, then set each element in the vector
//! to the index of the element, squared.

use gstd::{msg, prelude::*};

const SHORT: usize = 100;
const LONG: usize = 10000;

#[no_mangle]
extern "C" fn init() {
    let mut v = vec![0; SHORT];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
    msg::reply_bytes(format!("init: {}", v.into_iter().sum::<usize>()), 0).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    let mut v = vec![0; LONG];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
}
