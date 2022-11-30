// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

pub use demo_meta_io::*;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_METADATA;

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(feature = "std")]
pub use code::WASM_BINARY_META;

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::{Id, MessageIn, MessageInitIn, MessageInitOut, MessageOut, Wallet};
    use gstd::{msg, prelude::*};

    static mut WALLETS: Vec<Wallet> = Vec::new();

    #[no_mangle]
    unsafe extern "C" fn init() {
        WALLETS = Wallet::test_sequence();

        if msg::size() == 0 {
            return;
        }

        let message_init_in: MessageInitIn = msg::load().unwrap();
        let message_init_out: MessageInitOut = message_init_in.into();

        msg::reply(message_init_out, 0).unwrap();
    }

    #[no_mangle]
    unsafe extern "C" fn handle() {
        let message_in: MessageIn = msg::load().unwrap();

        let res = WALLETS
            .iter()
            .find(|w| w.id.decimal == message_in.id.decimal)
            .map(Clone::clone);

        let message_out = MessageOut { res };

        msg::reply(message_out, 0).unwrap();
    }

    #[no_mangle]
    extern "C" fn state() {
        msg::reply(unsafe { WALLETS.clone() }, 0).expect("Failed to share state");
    }

    #[no_mangle]
    extern "C" fn metahash() {
        let metahash: [u8; 32] = include!("../.metahash");
        msg::reply(metahash, 0).expect("Failed to share metahash");
    }

    #[no_mangle]
    extern "C" fn all_wallets() {
        let wallets: Vec<Wallet> = msg::load().unwrap();
        msg::reply(wallets, 0).expect("Failed to share state");
    }

    #[no_mangle]
    extern "C" fn specific_wallet() {
        let (id, wallets): (Id, Vec<Wallet>) = msg::load().unwrap();
        let res = wallets
            .into_iter()
            .filter(|w| w.id == id)
            .collect::<Vec<_>>();
        msg::reply(res, 0).expect("Failed to share state");
    }
}
