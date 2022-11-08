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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct InputArgs {
    pub destination: gstd::ActorId,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::InputArgs;
    use gstd::{msg, ActorId, ToString};

    static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);

    gstd::metadata! {
        title: "tests-proxy",
        handle:
            input: InputArgs,
    }

    #[no_mangle]
    unsafe extern "C" fn handle() {
        let payload = msg::load_bytes().expect("failed to load bytes");
        msg::send(DESTINATION, payload, msg::value());
    }

    #[no_mangle]
    unsafe extern "C" fn init() {
        let args: InputArgs = msg::load().expect("Failed to decode `InputArgs'");
        DESTINATION = args.destination;
    }
}
