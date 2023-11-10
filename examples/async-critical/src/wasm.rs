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

//! The program demonstrates asynchronous execution and
//! how to use macros `gstd::async_init`/`gstd::async_main`.
//!
//!  `Init` method gets three addresses, sends "PING" messages
//! to them and waits for at least two replies with any payload ("approvals").
//!
//! `Handle` processes only "PING" messages. When `handle` gets such message
//! it sends empty requests to the three addresses and waits for just one approval.
//! If an approval is obtained the method replies with "PONG".

use crate::HandleAction;
use gstd::{critical::Section, exec, msg, prelude::*};

#[gstd::async_init]
async fn init() {}

#[gstd::async_main]
async fn main() {
    let action: HandleAction = msg::load().expect("Failed to read handle action");

    match action {
        HandleAction::Normal => {
            let normal0 = Section::new(|| {
                msg::send_bytes(msg::source(), b"normal0", 0).unwrap();
            });

            let normal1 = Section::new(|| {
                msg::send_bytes(msg::source(), b"normal1", 0).unwrap();
            });

            normal0.execute();
            normal1.execute();
        }
        HandleAction::Panic => {
            // would not be executed
            let _before_panic = Section::new(|| {
                msg::send_bytes(msg::source(), b"before_panic", 0).unwrap();
            });

            panic!();
        }
        HandleAction::Wait => {
            let section = Section::new(|| {
                msg::send_bytes(msg::source(), b"before_wait", 0).unwrap();
            });

            gstd::msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");

            section.execute();
        }
        HandleAction::WaitAndPanic => {
            // call `gr_source` outside because it is forbidden in `handle_signal`
            let source = msg::source();
            let section = Section::new(move || {
                msg::send_bytes(source, b"before_wait", 0).unwrap();
            });

            gstd::msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");

            panic!();
        }
    }
}
