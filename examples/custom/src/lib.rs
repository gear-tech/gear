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

// TODO: #3058. Move here demo-vec, demo-ping, demo-distributor, demo-piggy-bank and others.
// Also need to make implementation with dyn instead of using matches.

#![no_std]

extern crate alloc;

pub mod backend_error;
pub mod btree;
pub mod capacitor;

use alloc::string::String;
use backend_error::BackendErrorState;
use btree::BTreeState;
use capacitor::CapacitorState;
use gstd::msg;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Decode, Encode)]
pub enum InitMessage {
    Capacitor(String),
    BTree,
    BackendError,
}

enum State {
    Capacitor(CapacitorState),
    BTree(BTreeState),
    BackendError(BackendErrorState),
}

static mut STATE: Option<State> = None;

#[no_mangle]
extern "C" fn init() {
    let init_message: InitMessage = msg::load_on_stack().expect("Failed to load payload bytes");
    let state = match init_message {
        InitMessage::Capacitor(payload) => State::Capacitor(capacitor::init(payload)),
        InitMessage::BTree => State::BTree(btree::init()),
        InitMessage::BackendError => State::BackendError(backend_error::init()),
    };
    unsafe { STATE = Some(state) };
}

#[no_mangle]
extern "C" fn handle() {
    let state = unsafe { STATE.as_mut().expect("State must be set in handle") };
    match state {
        State::Capacitor(state) => capacitor::handle(state),
        State::BTree(state) => btree::handle(state),
        _ => {}
    }
}

#[no_mangle]
extern "C" fn state() {
    let state = unsafe { STATE.take().expect("State must be set in handle") };
    if let State::BTree(state) = state {
        btree::state(state);
    }
}
