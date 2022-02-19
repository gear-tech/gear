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
        contract_a: Program {
            handle: ActorId::new([0u8; 32]),
        },
        contract_b: Program {
            handle: ActorId::new([0u8; 32]),
        },
    };

    struct State {
        contract_a: Program,
        contract_b: Program,
    }

    impl State {
        fn new(actor_a: impl Into<ActorId>, actor_b: impl Into<ActorId>) -> Self {
            Self {
                contract_a: Program::new(actor_a),
                contract_b: Program::new(actor_b),
            }
        }

        async fn compose(
            &mut self,
            input: Vec<u8>,
            gas_limit: u64,
        ) -> Result<Vec<u8>, &'static str> {
            debug!(
                "[0x{} compose::compose] Composing programs 0x{} and 0x{} on input {:?}",
                hex::encode(exec::program_id()),
                hex::encode(self.contract_a.handle),
                hex::encode(self.contract_b.handle),
                input
            );
            debug!(
                "[0x{} compose::compose] Calling contract 0x{} with gas limit {}",
                hex::encode(exec::program_id()),
                hex::encode(self.contract_a.handle),
                gas_limit / 2
            );
            let output_a = self.contract_a.call(input, gas_limit / 2).await?;
            debug!(
                "[0x{} compose::compose] Calling contract 0x{} with available gas: {}",
                hex::encode(exec::program_id()),
                hex::encode(self.contract_b.handle),
                gas_limit / 2
            );
            let output = self.contract_b.call(output_a, gas_limit / 2).await?;
            debug!(
                "[0x{} compose::compose] Output: {:?}",
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
                "[0x{} compose::Program::call] Received reply from remote contract: {:?}",
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
            "[0x{} compose::handle] input = {:?}, gas_available = {}",
            hex::encode(exec::program_id()),
            input,
            exec::gas_available()
        );

        if let Ok(outcome) =
            (unsafe { STATE.compose(input, exec::gas_available() - 100_000_000) }).await
        {
            debug!(
                "[0x{} compose::handle] Composition output: {:?}",
                hex::encode(exec::program_id()),
                outcome
            );
            msg::reply(outcome, exec::gas_available(), 0);
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let (contract_a, contract_b): (ActorId, ActorId) =
            msg::load().expect("Expecting two contract addresses");
        STATE = State::new(contract_a, contract_b);
        msg::reply((), 0, 0);
        debug!(
            "[0x{} compose::init] Program initialized",
            hex::encode(exec::program_id())
        );
    }
}
