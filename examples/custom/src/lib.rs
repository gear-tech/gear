// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

extern crate alloc;

pub mod backend_error;
pub mod btree;
pub mod capacitor;
pub mod distributor;
pub mod piggy_bank;
pub mod ping;
pub mod vec;

use alloc::string::String;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode)]
pub enum InitMessage {
    Capacitor(String),
    BTree,
    BackendError,
    Ping,
    Vec,
    PiggyBank,
    Distributor,
}

pub trait Program {
    fn init(args: gstd::Box<dyn gstd::any::Any>) -> Self
    where
        Self: Sized;

    fn handle(&'static mut self) {}
    fn state(&self) {}
}

#[cfg(not(feature = "std"))]
pub mod wasm {
    use super::{
        backend_error::wasm::BackendError, btree::wasm::BTree, capacitor::Capacitor,
        distributor::wasm::Distributor, piggy_bank::PiggyBank, ping::Ping, vec::Vec, InitMessage,
        Program,
    };
    use gstd::{msg, prelude::*, Box};

    static mut PROGRAM: Option<Box<dyn Program>> = None;

    #[no_mangle]
    extern "C" fn init() {
        let init_message: InitMessage = msg::load().expect("Failed to load payload bytes");

        let unit = Box::new(()); // empty arg for programs that don't need any args
        let program: Box<dyn Program> = match init_message {
            InitMessage::Capacitor(payload) => Box::new(Capacitor::init(Box::new(payload))),
            InitMessage::BTree => Box::new(BTree::init(unit)),
            InitMessage::BackendError => Box::new(BackendError::init(unit)),
            InitMessage::Ping => Box::new(Ping::init(unit)),
            InitMessage::Vec => Box::new(Vec::init(unit)),
            InitMessage::PiggyBank => Box::new(PiggyBank::init(unit)),
            InitMessage::Distributor => Box::new(Distributor::init(unit)),
        };
        unsafe { PROGRAM = Some(program) };
    }

    #[no_mangle]
    extern "C" fn handle() {
        let program = unsafe { PROGRAM.as_mut().expect("Program must be set at this point") };
        program.handle()
    }

    #[no_mangle]
    extern "C" fn state() {
        let program = unsafe { PROGRAM.take().expect("Program must be set at this point") };
        program.state()
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        gstd::record_reply();
    }
}
