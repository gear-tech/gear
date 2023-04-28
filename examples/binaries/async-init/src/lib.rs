// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use core::array::IntoIter;
use gstd::ActorId;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode)]
pub struct InputArgs {
    pub approver_first: ActorId,
    pub approver_second: ActorId,
    pub approver_third: ActorId,
}

impl InputArgs {
    pub fn from_two(first: impl Into<[u8; 32]>, second: impl Into<[u8; 32]>) -> Self {
        Self {
            approver_first: first.into().into(),
            approver_second: second.into().into(),
            approver_third: ActorId::zero(),
        }
    }

    pub fn iter(&self) -> IntoIter<&ActorId, 3> {
        [
            &self.approver_first,
            &self.approver_second,
            &self.approver_third,
        ]
        .into_iter()
    }
}

#[cfg(not(feature = "std"))]
mod wasm {
    /* The program demonstrates asynchronous execution and
     * how to use macros `gstd::async_init`/`gstd::async_main`.
     *
     * `Init` method gets three addresses, sends "PING" messages
     * to them and waits for at least two replies with any payload ("approvals").
     *
     * `Handle` processes only "PING" messages. When `handle` gets such message
     * it sends empty requests to the three addresses and waits for just one approval.
     * If an approval is obtained the method replies with "PONG".
     */

    use crate::InputArgs;
    use futures::future;
    use gstd::{msg, prelude::*, ActorId};

    // One of the addresses supposed to be non-program.
    static mut ARGUMENTS: InputArgs = InputArgs {
        approver_first: ActorId::zero(),
        approver_second: ActorId::zero(),
        approver_third: ActorId::zero(),
    };

    static mut RESPONSES: u8 = 0;

    #[gstd::async_init]
    async fn init() {
        let arguments: InputArgs = msg::load().expect("Failed to load arguments");

        let mut requests = arguments
            .iter()
            .map(|&addr| {
                msg::send_bytes_for_reply(addr, "PING", 0).expect("Failed to send message")
            })
            .collect::<Vec<_>>();

        unsafe {
            ARGUMENTS = arguments;
        }

        while !requests.is_empty() {
            let (.., remaining) = future::select_all(requests).await;
            unsafe {
                RESPONSES += 1;
            }

            if unsafe { RESPONSES } >= 2 {
                break;
            }

            requests = remaining;
        }
    }

    #[gstd::async_main]
    async fn main() {
        let message = msg::load_bytes().expect("Failed to load bytes");

        assert_eq!(message, b"PING");

        let requests = unsafe { ARGUMENTS.iter() }
            .map(|&addr| {
                msg::send_bytes_for_reply(addr, "PING", 0).expect("Failed to send message")
            })
            .collect::<Vec<_>>();

        let _ = future::select_all(requests).await;

        msg::reply(unsafe { RESPONSES }, 0).expect("Failed to send reply");
    }
}
