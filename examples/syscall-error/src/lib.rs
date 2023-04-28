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

use gstd::{msg, prelude::*, ActorId};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;
use gstd::errors::{ContractError, ExtError, MessageError};

#[no_mangle]
extern "C" fn init() {
    unsafe {
        let mut len = 0;
        gsys::gr_error(ptr::null_mut(), &mut len);
        assert_ne!(len, 0);

        let mut buf = vec![0; len as usize];
        let mut len = 0;
        gsys::gr_error(buf.as_mut_ptr(), &mut len);
        assert_eq!(len, 0);
        let err = ExtError::decode(&mut buf.as_ref()).unwrap();
        assert_eq!(err, ExtError::SyscallUsage);
    }

    let res = msg::send(ActorId::default(), "dummy", 250);
    assert_eq!(
        res,
        Err(ContractError::Ext(ExtError::Message(
            MessageError::InsufficientValue {
                message_value: 250,
                existential_deposit: 500
            }
        )))
    );
}

#[cfg(test)]
mod tests {
    extern crate std;

    use gtest::{Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send_bytes(0, b"dummy");
        assert!(!res.main_failed());
    }
}
