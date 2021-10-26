// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![no_std]
#![cfg_attr(target_arch = "wasm32", feature(alloc_error_handler))]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://gear-tech.io/images/logo-black.svg")]

extern crate galloc;

mod bail;
mod debug;
#[cfg(feature = "meta")]
pub mod meta;
pub mod msg;
pub mod prelude;

#[cfg(feature = "debug")]
pub use gcore::{exec, ext};
pub use gcore::{MessageId, ProgramId};

#[cfg(target_arch = "wasm32")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    #[cfg(feature = "debug")]
    {
        ext::debug("Runtime memory exhausted. Aborting");
    }
    core::arch::wasm32::unreachable()
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "debug")]
    {
        use galloc::prelude::*;

        let location_info = if let Some(location) = panic_info.location() {
            format!(", at: {}, {}", location.file(), location.line())
        } else {
            String::new()
        };

        if let Some(payload_str) = panic_info.payload().downcast_ref::<&str>() {
            ext::debug(&format!(
                "panic, payload: {:?}{}",
                payload_str, location_info
            ));
        }
    }
    core::arch::wasm32::unreachable();
}
