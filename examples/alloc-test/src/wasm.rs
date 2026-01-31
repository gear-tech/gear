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

use crate::{Action, Event};
use alloc::vec::Vec;
use gstd::{msg, prelude::*};

// Store allocated memory to prevent deallocation
static mut ALLOCATED: Vec<Vec<u8>> = Vec::new();
static mut TOTAL_SIZE: u32 = 0;

#[unsafe(no_mangle)]
extern "C" fn init() {
    // Empty init - program starts with no allocations
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let action: Action = msg::load().expect("Failed to decode Action");

    match action {
        Action::Alloc(size) => {
            // Allocate a vector of the requested size
            let data = vec![0u8; size as usize];

            // Store it to prevent deallocation
            unsafe {
                static_mut!(ALLOCATED).push(data);
                *static_mut!(TOTAL_SIZE) += size;
            }

            let total = unsafe { *static_ref!(TOTAL_SIZE) };
            msg::reply(Event::Allocated(total), 0).expect("Failed to reply");
        }
        Action::GetAllocatedSize => {
            let total = unsafe { *static_ref!(TOTAL_SIZE) };
            msg::reply(Event::AllocatedSize(total), 0).expect("Failed to reply");
        }
    }
}
