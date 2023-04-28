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

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

extern crate alloc;

use codec::{Decode, Encode};
use gstd::prelude::*;

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Request {
    Insert(u32, u32),
    Remove(u32),
    List,
    Clear,
}

#[derive(Encode, Debug, Decode, PartialEq, Eq)]
pub enum Reply {
    Error,
    None,
    Value(Option<u32>),
    List(Vec<(u32, u32)>),
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;

    use alloc::collections::BTreeMap;
    use gstd::{debug, msg};

    static mut STATE: Option<BTreeMap<u32, u32>> = None;

    #[no_mangle]
    extern "C" fn handle() {
        let reply = match msg::load() {
            Ok(request) => process(request),
            Err(e) => {
                debug!("Error processing request: {:?}", e);
                Reply::Error
            }
        };

        msg::reply(reply, 0).unwrap();
    }

    fn state() -> &'static mut BTreeMap<u32, u32> {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: Request) -> Reply {
        use Request::*;

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
    extern "C" fn init() {
        unsafe { STATE = Some(BTreeMap::new()) };
        msg::reply((), 0).unwrap();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{Reply, Request};
    use alloc::vec;
    use gtest::{Log, Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let res = program.send_bytes(from, b"init");
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn simple() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = 42;

        let _res = program.send_bytes(from, b"init");

        IntoIterator::into_iter([
            Request::Insert(0, 1),
            Request::Insert(0, 2),
            Request::Insert(1, 3),
            Request::Insert(2, 5),
            Request::Remove(1),
            Request::List,
            Request::Clear,
            Request::List,
        ])
        .map(|r| program.send(from, r))
        .zip(IntoIterator::into_iter([
            Reply::Value(None),
            Reply::Value(Some(1)),
            Reply::Value(None),
            Reply::Value(None),
            Reply::Value(Some(3)),
            Reply::List(vec![(0, 2), (2, 5)]),
            Reply::None,
            Reply::List(vec![]),
        ]))
        .for_each(|(result, reply)| {
            let log = Log::builder()
                .source(program.id())
                .dest(from)
                .payload(reply);
            assert!(result.contains(&log));
        })
    }
}
