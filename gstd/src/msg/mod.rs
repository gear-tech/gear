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

//! Messaging API for Gear programs.
//!
//! This module contains an API to process incoming messages and send outgoing
//! ones. Messages are the primary communication interface between actors (users
//! and programs).
//!
//! Every Gear program has code that handles messages. During message
//! processing, a program can send messages to other programs and users,
//! including a reply to the initial message.
//!
//! When some actor (user or program) sends a message to the program, it invokes
//! this program by executing the `handle` function. The invoked program can
//! obtain details of incoming messages by using this module's API ([`source`],
//! [`size`], [`load`], [`id`], [`value`], etc.).
//!
//! Optionally the program can send one or more messages to other actors. Also,
//! it can send a reply that differs from a regular message in two ways:
//! - There can be no more than one reply;
//! - It is impossible to choose the reply's destination, as it is always sent
//!   to the program invoker.
//!
//! Note that messages and a reply are not sent immediately but collected during
//! the program execution and enqueued after the execution successfully ends.

#[macro_use]
mod macros;

mod r#async;
pub use r#async::*;

mod basic;
pub use basic::*;

mod encoded;
pub use encoded::*;

mod utils;
