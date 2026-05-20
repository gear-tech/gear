// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloc::alloc::Layout;

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe {
        // Force rustc not to remove memory import
        *(10usize as *mut u8) = 10;
    }
    alloc::alloc::handle_alloc_error(Layout::new::<[u8; 64 * 1024]>());
}
