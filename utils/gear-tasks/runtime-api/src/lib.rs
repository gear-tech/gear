// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;

sp_api::decl_runtime_apis! {
    pub trait GearTasksApi {
        fn execute_task(func_ref: u64, payload: Vec<u8>) -> Vec<u8>;
    }
}

pub fn impl_fn(func_ref: u64, payload: Vec<u8>) -> Vec<u8> {
    #[cfg(target_arch = "wasm32")]
    {
        let f = unsafe { core::mem::transmute::<u32, fn(Vec<u8>) -> Vec<u8>>(func_ref as u32) };
        f(payload)
    }

    #[cfg(feature = "std")]
    {
        let _ = (func_ref, payload);
        unreachable!("`gear-tasks` uses different implementation for native calls")
    }
}
