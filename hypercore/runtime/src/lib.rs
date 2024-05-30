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

#[cfg(feature = "export")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "export")]
pub use code::WASM_BINARY;

#[cfg(not(feature = "export"))]
mod wasm {
    extern crate alloc;
    extern crate galloc;

    use alloc::{format, vec::Vec};
    use core::{mem::transmute, panic::PanicInfo, ptr};
    use gprimitives::ActorId as ProgramId;

    mod sys {
        use super::*;

        extern "C" {
            pub fn debug(data: *const u8, len: u32);
        }
    }

    #[no_mangle]
    extern "C" fn greet() {
        let message = "Hello, world!";
        unsafe { sys::debug(message.as_ptr(), message.len() as u32) };
    }

    #[panic_handler]
    fn panic_handler(_: &PanicInfo) -> ! {
        core::arch::wasm32::unreachable()
    }
}
