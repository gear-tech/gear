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

// This program recursively composes itself with another program (the other program
// being applied to the input data first): `c(f) = (c(f) . f) x`.
// Every call to the auto_composer program increments the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` programs `f`
// where the output of the previous call is fed to the input of the next call.

use gstd::{ActorId, debug, exec, msg, prelude::*};

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

        // Pass the input to the `other` program first
        debug!(
            "[0x{} ncompose::compose_with_self] Calling program 0x{}",
            hex::encode(self.me.handle),
            hex::encode(self.other.handle)
        );
        let output_other = self.other.call(input).await?;
        debug!(
            "[0x{} ncompose::compose_with_self] Calling self",
            hex::encode(exec::program_id())
        );
        let output = self.me.call(output_other).await?;
        debug!(
            "[0x{} ncompose::compose_with_self] Composition output: {output:?}",
            hex::encode(exec::program_id()),
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

    async fn call(&self, input: Vec<u8>) -> Result<Vec<u8>, &'static str> {
        let reply_bytes = msg::send_bytes_for_reply(self.handle, &input[..], 0, 0)
            .expect("Error sending message")
            .await
            .map_err(|_| "Error in async message processing")?;
        debug!(
            "[0x{} ncompose::Program::call] Received reply from remote program: {:?}",
            hex::encode(exec::program_id()),
            hex::encode(&reply_bytes)
        );

        Ok(reply_bytes)
    }
}

#[gstd::async_main]
async fn main() {
    let input = msg::load_bytes().expect("Failed to load payload bytes");
    debug!(
        "[0x{} ncompose::handle] input = {input:?}",
        hex::encode(unsafe { STATE.me.handle }),
    );

    if let Ok(outcome) = (unsafe { static_mut!(STATE).compose_with_self(input) }).await {
        debug!(
            "[0x{} ncompose::handle] Composition output: {outcome:?}",
            hex::encode(exec::program_id()),
        );
        msg::reply(outcome, 0).unwrap();
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let (actor, max_iter): (ActorId, u16) =
        msg::load().expect("Malformed input: expecting a program ID and a number");
    unsafe { STATE = State::new(max_iter, actor) };
    msg::reply_bytes([], 0).unwrap();
    debug!(
        "[0x{} ncompose::init] Program initialized",
        hex::encode(exec::program_id())
    );
}
