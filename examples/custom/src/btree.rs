// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::{Decode, Encode};

use alloc::vec::Vec;

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

#[derive(Debug, Encode, Decode)]
pub enum StateRequest {
    Full,
    ForKey(u32),
}

#[cfg(not(feature = "std"))]
pub(crate) mod wasm {
    use super::*;
    use gstd::{collections::BTreeMap, debug, msg, prelude::*};

    pub(crate) type State = BTreeMap<u32, u32>;

    pub(crate) fn init() -> State {
        msg::reply((), 0).unwrap();
        BTreeMap::new()
    }

    pub(crate) fn handle(state: &mut State) {
        let reply = msg::load()
            .map(|request| process(state, request))
            .unwrap_or_else(|e| {
                debug!("Error processing request: {e:?}");
                Reply::Error
            });
        msg::reply(reply, 0).unwrap();
    }

    pub(crate) fn state(state: State) {
        let request: StateRequest = msg::load().unwrap();
        match request {
            StateRequest::Full => msg::reply(state, 0).unwrap(),
            StateRequest::ForKey(key) => msg::reply(state.get(&key), 0).unwrap(),
        };
    }

    fn process(state: &mut State, request: Request) -> Reply {
        use Request::*;

        match request {
            Insert(key, value) => Reply::Value(state.insert(key, value)),
            Remove(key) => Reply::Value(state.remove(&key)),
            List => Reply::List(state.iter().map(|(k, v)| (*k, *v)).collect()),
            Clear => {
                state.clear();
                Reply::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{Reply, Request};
    use crate::InitMessage;
    use alloc::vec;
    use gtest::{Log, Program, System, constants::DEFAULT_USER_ALICE};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let from = DEFAULT_USER_ALICE;

        program.send(from, InitMessage::BTree);
        let res = system.run_next_block();
        let log = Log::builder().source(program.id()).dest(from);
        assert!(res.contains(&log));
    }

    #[test]
    fn simple() {
        let system = System::new();
        system.init_logger();

        let program = Program::current_opt(&system);

        let from = DEFAULT_USER_ALICE;

        program.send(from, InitMessage::BTree);
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
        .map(|r| {
            program.send(from, r);
            system.run_next_block()
        })
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
