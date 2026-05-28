// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
