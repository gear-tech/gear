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

// This contract recursively composes itself with another contract (the other contract
// being applied to the input data first): `c(f) = (c(f) . f) x`.
// Every call to the auto_composer contract incremets the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` contracts `f`
// where the output of the previous call is fed to the input of the next call.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use native::{WASM_BINARY, WASM_BINARY_BLOATY};

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use codec::{Decode, Encode};
    use gstd::{debug, exec, msg, prelude::*, ActorId};

    static mut STATE: State = State {
        iter: 0,
        max_iter: 0,
        me: Program {
            handle: ActorId::new([0u8; 32]),
        },
        other: Program {
            handle: ActorId::new([0u8; 32]),
        },
    };

    struct State {
        iter: u16,
        max_iter: u16,
        me: Program,
        other: Program,
    }

    impl State {
        fn new(max_iter: u16, actor: impl Into<ActorId>) -> Self {
            Self {
                iter: 0,
                max_iter,
                me: Program::new(exec::program_id()),
                other: Program::new(actor),
            }
        }

        fn inc(&mut self) {
            self.iter += 1;
        }

        async fn compose_with_self(&mut self, input: Vec<u8>) -> Result<Vec<u8>, &'static str> {
            if self.iter >= self.max_iter {
                debug!(
                    "[0x{} ncompose::compose_with_self] Max number of iterations {} reached; no further progress is possible",
                    hex::encode(self.me.handle),
                    self.max_iter
                );
                return Err("Max iteration reached");
            }
            // Increase iter
            self.inc();

            debug!(
                "[ncompose::compose_with_self] Iter: {} out of {}",
                self.iter, self.max_iter
            );

            // Pass the input to the `other` contract first
            debug!(
                "[0x{} ncompose::compose_with_self] Calling contract 0x{} with available gas {}",
                hex::encode(self.me.handle),
                hex::encode(self.other.handle),
                exec::gas_available(),
            );
            let output_other = self
                .other
                .call(input, exec::gas_available() - 200_000_000)
                .await?;
            debug!(
                "[0x{} ncompose::compose_with_self] Calling self with gas limit {}",
                hex::encode(exec::program_id()),
                exec::gas_available(),
            );
            let output = self
                .me
                .call(output_other, exec::gas_available() - 200_000_000)
                .await?;
            debug!(
                "[0x{} ncompose::compose_with_self] Output: {:?}",
                hex::encode(exec::program_id()),
                output
            );

            Ok(output)
        }
    }

    #[derive(Eq, Ord, PartialEq, PartialOrd)]
    struct Program {
        handle: ActorId,
    }

    impl Program {
        fn new(handle: impl Into<ActorId>) -> Self {
            Self {
                handle: handle.into(),
            }
        }

        async fn call(&self, input: Vec<u8>, gas_limit: u64) -> Result<Vec<u8>, &'static str> {
            let reply_bytes =
                msg::send_bytes_and_wait_for_reply(self.handle, &input[..], gas_limit, 0)
                    .await
                    .map_err(|_| "Error in async message processing")?;
            debug!(
                "[0x{} ncompose::Program::call] Received reply from remote contract: {:?}",
                hex::encode(exec::program_id()),
                reply_bytes
            );

            Ok(reply_bytes)
        }
    }

    #[gstd::async_main]
    async fn main() {
        let input = msg::load_bytes();
        debug!(
            "[0x{} ncompose::handle] input = {:?}, available gas: {}",
            hex::encode(unsafe { STATE.me.handle }),
            input,
            exec::gas_available()
        );

        if let Ok(outcome) = (unsafe { STATE.compose_with_self(input) }).await {
            debug!(
                "[0x{} ncompose::handle] Composition output: {:?}",
                hex::encode(exec::program_id()),
                outcome
            );
            msg::reply(outcome, exec::gas_available(), 0);
            // msg::send_bytes(
            //     exec::program_id(),
            //     outcome.encode(),
            //     exec::gas_available() - 200_000_000,
            //     0,
            // );
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let (actor, max_iter): (ActorId, u16) =
            msg::load().expect("Malformed input: expecting a program ID and a number");
        STATE = State::new(max_iter, actor);
        msg::reply((), 0, 0);
        debug!(
            "[0x{} ncompose::init] Program initialized",
            hex::encode(exec::program_id())
        );
    }
}
