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

#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::prelude::*;

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    Insert(u32, u32),
    Remove(u32),
    List,
    Clear,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Error,
    None,
    Value(Option<u32>),
    List(Vec<(u32, u32)>),
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::BTreeMap;
    use codec::{Decode, Encode};
    use gstd::{debug, msg, prelude::*};

    use super::{Reply, Request};

    static mut STATE: Option<BTreeMap<u32, u32>> = None;

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let reply = match msg::load() {
            Ok(request) => process(request),
            Err(e) => {
                debug!("Error processing request: {:?}", e);
                Reply::Error
            }
        };

        msg::reply(reply, 1_000_000, 0);
    }

    fn state() -> &'static mut BTreeMap<u32, u32> {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: super::Request) -> Reply {
        use super::Request::*;
        match request {
            Insert(key, value) => Reply::Value(state().insert(key, value)),
            Remove(key) => Reply::Value(state().remove(&key)),
            List => Reply::List(state().iter().map(|(k, v)| (*k, *v)).collect()),
            Clear => {
                state().clear();
                Reply::None
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        STATE = Some(BTreeMap::new());
        msg::reply((), 0, 0);
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::native;
    use super::{Reply, Request};

    use common::{InitProgram, RunnerContext};

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY_BLOATY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = RunnerContext::default();

        // Assertions are performed when decoding reply
        let _reply: () =
            runner.init_program_with_reply(InitProgram::from(wasm_code()).message(b"init"));
    }

    #[test]
    fn simple() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let reply: Vec<Reply> = runner.request_batch(&[
            Request::Insert(0, 1),
            Request::Insert(0, 2),
            Request::Insert(1, 3),
            Request::Insert(2, 5),
            Request::Remove(1),
            Request::List,
            Request::Clear,
            Request::List,
        ]);
        assert_eq!(
            reply,
            &[
                Reply::Value(None),
                Reply::Value(Some(1)),
                Reply::Value(None),
                Reply::Value(None),
                Reply::Value(Some(3)),
                Reply::List(vec![(0, 2), (2, 5)]),
                Reply::None,
                Reply::List(vec![]),
            ],
        );
    }
}
