// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gstd::{prelude::*, static_mut, static_ref};

static mut CTORS: u64 = 0;
static mut DTORS: u64 = 0;

gstd::ctor! {
    unsafe extern "C" fn() {
        *static_mut!(CTORS) += 1;
    }
}

gstd::dtor! {
    unsafe extern "C" fn() {
        *static_mut!(DTORS) += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe {
        assert_eq!(*static_mut!(CTORS), 1);
        assert_eq!(*static_ref!(DTORS), 0);
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    unsafe {
        assert_eq!(*static_ref!(CTORS), 2);
        assert_eq!(*static_ref!(DTORS), 1);
    }
}
