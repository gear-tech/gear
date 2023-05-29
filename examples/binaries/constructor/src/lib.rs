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

// This contract represents a general purpose contract that can perform a number
// of actions in a loop, one action per loop iteration. Which of the 4 actions:
// `Exec`, `Send` (to a program), `Send` (to a non-program) or `Trap` should be executed
// is determined by some random value derived from the `handle` input.

#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

use parity_scale_codec::{Codec, Decode, Encode};

pub type Key = String;

#[derive(Clone, Debug, Decode, Encode)]
pub enum Arg<T: 'static + Clone + Codec> {
    New(T),
    Get(Key),
}

impl<T: 'static + Clone + Codec> From<T> for Arg<T> {
    fn from(value: T) -> Self {
        Arg::New(value)
    }
}

#[derive(Clone, Debug, Decode, Encode)]
pub enum Call {
    Store(Key),
    StoreVec(Key),
    Source,
    Send(Arg<[u8; 32]>, Arg<Vec<u8>>, Option<u64>, u128, u32),
    Panic(Option<String>),
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{debug, msg, prelude::*};

    type Value = Vec<u8>;
    type CallResult = (Call, Option<Value>);

    static mut DATA: BTreeMap<Key, Value> = BTreeMap::new();

    impl<T: 'static + Clone + Codec> Arg<T> {
        pub fn get(self) -> T {
            match self {
                Self::New(value) => value,
                Self::Get(key) => {
                    let value = unsafe { DATA.get(&key) }
                        .unwrap_or_else(|| panic!("Value in key {key} doesn't exist"));
                    T::decode(&mut value.as_ref())
                        .unwrap_or_else(|_| panic!("Value in key {key} failed decode"))
                }
            }
        }
    }

    impl Call {
        fn store_impl(
            self,
            key: Key,
            previous: Option<CallResult>,
            extra_encode: bool,
        ) -> Option<Value> {
            let (call, value) = previous.unwrap_or_else(|| {
                panic!("Call <{self:?}> couldn't be called without previous call")
            });

            let value = value.unwrap_or_else(|| {
                panic!("Call <{self:?}> couldn't be called after no-output call <{call:?}>")
            });

            let value = extra_encode.then(|| value.encode()).unwrap_or(value);

            debug!(
                "\t[CONSTRUCTOR] >> Storing {:?}: {:?}",
                key,
                &value[extra_encode as usize..]
            );

            unsafe { DATA.insert(key, value) };

            None
        }

        fn store(self, previous: Option<CallResult>) -> Option<Value> {
            let Self::Store(key) = self.clone() else { unreachable!() };

            self.store_impl(key, previous, false)
        }

        fn store_vec(self, previous: Option<CallResult>) -> Option<Value> {
            let Self::StoreVec(key) = self.clone() else { unreachable!() };

            self.store_impl(key, previous, true)
        }

        fn source(self) -> Option<Value> {
            (!matches!(self, Self::Source)).then(|| unreachable!());

            Some(msg::source().encode())
        }

        fn panic(self) -> ! {
            let Self::Panic(msg) = self else { unreachable!() };

            if let Some(msg) = msg {
                panic!("{msg}");
            } else {
                panic!();
            }
        }

        fn send(self) -> Option<Value> {
            let Self::Send(destination, payload, gas_limit, value, delay) = self else { unreachable!() };

            let destination = destination.get().into();
            let payload = payload.get();

            let res = if let Some(gas_limit) = gas_limit {
                msg::send_bytes_with_gas_delayed(destination, payload, gas_limit, value, delay)
            } else {
                msg::send_bytes_delayed(destination, payload, value, delay)
            };

            let message_id = res.expect("Failed to send message");

            Some(message_id.encode())
        }

        fn process(self, previous: Option<CallResult>) -> CallResult {
            debug!("\t[CONSTRUCTOR] >> Processing {:?}", self);
            let call = self.clone();

            let value = match self {
                Call::Store(..) => self.store(previous),
                Call::StoreVec(..) => self.store_vec(previous),
                Call::Source => self.source(),
                Call::Panic(..) => self.panic(),
                Call::Send(..) => self.send(),
            };

            (call, value)
        }
    }

    fn process(calls: Vec<Call>) {
        let mut res = None;

        for call in calls {
            res = Some(call.process(res));
        }
    }

    #[no_mangle]
    extern "C" fn init() {
        let calls = msg::load().expect("Failed to load payload");

        process(calls)
    }

    #[no_mangle]
    extern "C" fn handle() {
        let calls = msg::load().expect("Failed to load payload");

        process(calls)
    }
}
