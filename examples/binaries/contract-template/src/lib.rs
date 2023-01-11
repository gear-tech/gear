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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use gstd::{debug, exec, msg, prelude::*, ActorId};
    use rand::{rngs::StdRng, RngCore, SeedableRng};

    const MAX_ITER: u8 = 5;
    const MAX_COMPLEXITY: u32 = 100;

    #[derive(PartialEq)]
    pub enum Action {
        Exec(u32),
        Send(ActorId, Vec<u8>),
        Trap,
    }

    impl core::fmt::Debug for Action {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self {
                Action::Exec(k) => write!(f, "Action::Exec({})", k),
                Action::Send(addr, payload) => write!(
                    f,
                    "Action::Send{{ actor = 0x{}, data = 0x{} }}",
                    hex::encode(addr.as_ref()),
                    hex::encode(payload),
                ),
                _ => write!(f, "Action::Trap"),
            }
        }
    }

    static mut STATE: State = State {
        programs: vec![],
        non_programs: vec![],
        me: ActorId::new([0u8; 32]),
    };

    struct State {
        programs: Vec<ActorId>,
        non_programs: Vec<ActorId>,
        me: ActorId,
    }

    impl State {
        fn new(programs: Vec<ActorId>) -> Self {
            let num_contracts = programs.len();
            let mut non_programs = Vec::<ActorId>::new();

            if num_contracts > 0 {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&programs[0].as_ref()[..32]);
                let mut rng: StdRng = SeedableRng::from_seed(seed);
                let mut buffer = [0u8; 32];
                for _ in 0..num_contracts {
                    rng.fill_bytes(&mut buffer);
                    non_programs.push(buffer.into());
                }
            }
            Self {
                programs,
                non_programs,
                me: exec::program_id(),
            }
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

        async fn call(&self, input: Vec<u8>) -> Result<Vec<u8>, &'static str> {
            let reply_bytes = msg::send_bytes_for_reply(self.handle, &input[..], 0)
                .expect("Error sending message")
                .await
                .map_err(|_| "Error in async message processing")?;
            debug!(
                "[0x{} contract-template::Program::call] Received reply from remote contract 0x{}: 0x{}",
                hex::encode(unsafe { STATE.me }),
                hex::encode(self.handle),
                hex::encode(&reply_bytes)
            );

            Ok(reply_bytes)
        }
    }

    async fn dummy_fn(n: u32) -> u32 {
        let mut s = 0_u32;
        for i in 0..=n * 1000 {
            s += i;
        }
        s
    }

    #[gstd::async_main]
    async fn main() {
        let input = msg::load_bytes().expect("Failed to load payload bytes");
        debug!(
            "[0x{} contract_template::handle] input = 0x{}, exec::gas_available(): {}",
            hex::encode(unsafe { STATE.me }),
            hex::encode(&input),
            exec::gas_available()
        );

        let mut seed = [0u8; 32];
        seed.copy_from_slice(&input[..32]);
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut bytes = [0u8; (MAX_ITER * 40) as usize];
        rng.fill_bytes(&mut bytes);

        let mut entropy = &bytes[..];

        // Select number of iterations
        let num_iter = entropy[0] % MAX_ITER + 1; // Ruling out 0 iterations case
        debug!(
            "[0x{} contract_template::handle] taking up to {} action(s)",
            hex::encode(unsafe { STATE.me }),
            num_iter
        );
        entropy = &entropy[1..];

        for _iter in 0..num_iter {
            // Select action
            let c = entropy[0];
            entropy = &entropy[1..];
            let action = match c {
                0..=108 => {
                    let mut val = [0_u8; 4];
                    val.copy_from_slice(&entropy[0..4]);
                    let n = u32::from_le_bytes(val) % MAX_COMPLEXITY + 1;
                    entropy = &entropy[4..];
                    Action::Exec(n)
                }
                109..=216 => {
                    let mut val = [0_u8; 4];
                    val.copy_from_slice(&entropy[0..4]);
                    let programs = unsafe { &STATE.programs };
                    let i = (u32::from_le_bytes(val) as usize) % programs.len();

                    let mut val = [0_u8; 32];
                    val.copy_from_slice(&entropy[4..36]);
                    let payload = val.to_vec();

                    entropy = &entropy[36..];
                    Action::Send(programs[i], payload)
                }
                217..=232 => {
                    let mut val = [0u8; 4];
                    val.copy_from_slice(&entropy[0..4]);
                    let addrs = unsafe { &STATE.non_programs };
                    let i = (u32::from_le_bytes(val) as usize) % addrs.len();
                    entropy = &entropy[4..];
                    Action::Send(addrs[i], Vec::new())
                }
                _ => Action::Trap,
            };

            debug!(
                "[0x{} contract_template::handle] Running action {:?}",
                hex::encode(unsafe { STATE.me }),
                action
            );

            match action {
                Action::Exec(k) => {
                    dummy_fn(k).await;
                }
                Action::Send(addr, payload) => {
                    let _ = Program::new(addr).call(payload).await;
                }
                Action::Trap => {
                    panic!("Panic in contract");
                }
            };
        }

        msg::reply_bytes(b"Success", 0).unwrap();
    }

    #[no_mangle]
    extern "C" fn init() {
        let programs: Vec<ActorId> =
            msg::load().expect("Malformed input: expecting vectors of program IDs and random IDs");
        unsafe { STATE = State::new(programs) };
        msg::reply_bytes([], 0).unwrap();
        debug!(
            "[0x{} contract-template::init] Program initialized",
            hex::encode(exec::program_id())
        );
    }
}
