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

//! # Testing with `gtest`
//!
//! `gtest` simulates a real network by providing mockups of the user, program,
//! balances, mailbox, etc. Since it does not include parts of the actual
//! blockchain, it is fast and lightweight. But being a model of the blockchain
//! network, `gtest` cannot be a complete reflection of the latter.
//!
//! As we said earlier, `gtest` is excellent for unit and integration testing.
//! It is also helpful for debugging Gear program logic. Nothing other than the
//! Rust compiler is required for running tests based on `gtest`. It is
//! predictable and robust when used in continuous integration.
//!
//! ## Main concepts
//!
//! `gtest` is a library that provides a set of tools for testing Gear programs. The most important structures are:
//!
//! - [`System`] — a structure that represents the environment of the Gear network. It contains the current block number, timestamp, and other parameters. It also stores the mailbox and the list of programs.
//! - [`Program`] — a structure that represents a Gear program. It contains the information about program and allows sending messages to other programs.
//! - [`Log`] — a structure that represents a message log. It allows check the result of the program execution.
//!
//! Let's take a closer look at how to write tests using `gtest`.
//!
//! ## Import `gtest` lib
//!
//! To use the `gtest` library, you must import it into your `Cargo.toml` file
//! in the `[dev-dependencies]` block to fetch and compile it for tests only:
//!
//! ```toml
//! [package]
//! name = "my-gear-app"
//! version = "0.1.0"
//! authors = ["Your Name"]
//! edition = "2021"
//!
//! [dependencies]
//! gstd = { git = "https://github.com/gear-tech/gear.git", tag = "v1.0.1" }
//!
//! [build-dependencies]
//! gear-wasm-builder = { git = "https://github.com/gear-tech/gear.git", tag = "v1.0.1" }
//!
//! [dev-dependencies]
//! gtest = { git = "https://github.com/gear-tech/gear.git", tag = "v1.0.1" }
//! ```
//!
//! ## Program example
//!
//! Let's write a simple program that will receive a message and reply to it.
//!
//! `lib.rs`:
//!
//! ```
//! #![no_std]
//! use gstd::msg;
//!
//! #[no_mangle]
//! extern "C" fn handle() {
//!     let payload = msg::load_bytes().expect("Failed to load payload");
//!
//!     if payload == b"PING" {
//!         msg::reply_bytes(b"PONG", 0).expect("Failed to send reply");
//!     }
//! }
//! ```
//!
//! `build.rs`:
//!
//! ```
//! fn main() {
//!     gear_wasm_builder::build();
//! }
//! ```
//!
//! We will add a test that will check the program's behavior. To do this, we will use the `gtest` library.
//!
//! Our test will consist of the following steps:
//!
//! 1. Initialize the `System` structure.
//! 2. Initialize the `Program` structure.
//! 3. Send an init message to the program. We don't have an `init` function in our program, but the first message to the program is always the init message while using `gtest`.
//! 4. Send a handle message to the program.
//! 5. Check the result of the program execution.
//!
//! Add these lines to the bottom of the `lib.rs` file:
//!
//! ```
//! #[cfg(test)]
//! mod tests {
//!     use gtest::{Log, Program, System};
//!
//!     const USER_ID: u64 = 100001;
//!
//!     #[test]
//!     fn test_ping_pong() {
//!         // Initialization of the common environment for running smart contracts.
//!         let sys = System::new();
//!
//!         // Initialization of the current program structure.
//!         let prog = Program::current(&sys);
//!
//!         // Send an init message to the program.
//!         let res = prog.send_bytes(USER_ID, b"Doesn't matter");
//!
//!         // Check whether the program was initialized successfully.
//!         assert!(!res.main_failed());
//!
//!         // Send a handle message to the program.
//!         let res = prog.send_bytes(USER_ID, b"PING");
//!
//!         // Check the result of the program execution.
//!         // 1. Create a log with the expected result.
//!         let log = Log::builder()
//!             .source(prog.id())
//!             .dest(USER_ID)
//!             .payload_bytes(b"PONG");
//!
//!         // 2. Check whether the program was executed successfully.
//!         assert!(!res.main_failed());
//!
//!         // 3. Make sure the log entry is in the result.
//!         assert!(res.contains(&log));
//!     }
//! }
//! ```
//!
//! To run the test, use the following command:
//!
//! ```bash
//! cargo test
//! ```
//!
//! # `gtest` capabilities
//!
//! Let's take a closer look at the `gtest` capabilities.
//!
//! ## Initialization of the common environment for running smart contracts
//!
//! ```
//! let sys = System::new();
//! ```
//!
//! This emulates node's and chain's behavior. By default, the [`System::new`] function sets the following parameters:
//!
//! - current block equals `0`
//! - current timestamp equals UNIX timestamp of your system
//! - starting message id equals `0x010000..`
//! - starting program id equals `0x010000..`
//!
//! ## Program initialization
//!
//! There are several ways to initialize a program:
//!
//! - Initialize the current program using the [`Program::current`] function:
//!
//!     ```
//!     let prog = Program::current(&sys);
//!     ```
//!
//! - Initialize a program from a Wasm-file with a default id using the [`Program::from_file`] function:
//!
//!     ```
//!     let prog = Program::from_file(
//!         &sys,
//!         "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
//!     );
//!     ```
//!
//! - Initialize a program from a Wasm-file with a custom id using the [`Program::from_file_with_id`] function:
//!
//!     ```
//!     let prog = Program::from_file_with_id(
//!         &sys,
//!         105,
//!         "./target/wasm32-unknown-unknown/release/demo_ping.wasm",
//!     );
//!     ```
//!
//!     Every place in this lib, where you need to specify some ids, it requires generic type `ID`, which implements ``Into<ProgramIdWrapper>``.
//!
//!     `ProgramIdWrapper` may be built from:
//!     - `u64`
//!     - `[u8; 32]`
//!     - `String`
//!     - `&str`
//!     - [`ProgramId`](https://docs.gear.rs/gear_core/ids/struct.ProgramId.html) (from `gear_core` one's, not from `gstd`).
//!
//!     `String` implementation means the input as hex (with or without "0x").
//!
//! ## Getting the program from the system
//!
//! If you initialize program not in this scope, in cycle, in other conditions, where you didn't save the structure, you may get the object from the system by id.
//!
//! ```
//! let prog = sys.get_program(105);
//! ```
//!
//! ## Initialization of styled `env_logger`
//!
//! Initialization of styled `env_logger` to print logs (only from `gwasm` by default) into stdout:
//!
//! ```
//! sys.init_logger();
//! ```
//!
//! To specify printed logs, set the env variable `RUST_LOG`:
//!
//! ```bash
//! RUST_LOG="target_1=logging_level,target_2=logging_level" cargo test
//! ```
//!
//! ## Sending messages
//!
//! To send message to the program need to call one of two program's functions:
//!
//! - [`Program::send`] (or [`Program::send_with_value`] if you need to send a message with attached funds).
//! - [`Program::send_bytes`] (or [`Program::send_bytes_with_value`] if you need to send a message with attached funds).
//!
//! Both of the methods require sender id as the first argument and the payload as second.
//!
//! The difference between them is pretty simple and similar to [`gstd`](https://docs.gear.rs/gstd/) functions [`msg::send`](https://docs.gear.rs/gstd/msg/fn.send.html) and [`msg::send_bytes`](https://docs.gear.rs/gstd/msg/fn.send_bytes.html).
//!
//! The first one requires payload to be CODEC Encodable, while the second requires payload implement `AsRef<[u8]>`, that means to be able to represent as bytes.
//!
//! [`Program::send`] uses [`Program::send_bytes`] under the hood with bytes from `payload.encode()`.
//!
//! First message to the initialized program structure is always the init message.
//!
//! ```
//! let res = program.send_bytes(100001, "INIT MESSAGE");
//! ```
//!
//! ## Processing the result of the program execution
//!
//! Any sending functions in the lib returns [`RunResult`] structure.
//!
//! It contains the final result of the processing message and others, which were created during the execution.
//!
//! It has 4 main functions:
//!
//! - [`RunResult::log`] — returns the reference to the Vec produced to users messages. You may assert them as you wish, iterating through them.
//! - [`RunResult::main_failed`] — returns bool which shows that there was panic during the execution of the main message.
//! - [`RunResult::others_failed`] — returns bool which shows that there was panic during the execution of the created messages during the main execution. Equals false if no others were called.
//! - [`RunResult::contains`] — returns bool which shows that logs contain a given log. Syntax sugar around `res.log().iter().any(|v| v == arg)`.
//!
//! To build a log for assertion you need to use [`Log`] structure with its builders. All fields here are optional. Assertion with `Log`s from core are made on the `Some(..)` fields. You will run into panic if you try to set the already specified field.
//!
//!
//!
//! - Processing the result of the program execution:
//! ```ignore
//!     // Any sending functions in the lib returns `RunResult` structure.
//!     //
//!     // It contains the final result of the processing message and others,
//!     // which were created during the execution.
//!     //
//!     // It has 4 main functions.
//!
//!     // Returns the reference to the Vec produced to users messages.
//!     // You may assert them as you wish, iterating through them.
//!     assert!(res.log().is_empty());
//!
//!     // Returns bool which shows that there was panic during the execution
//!     // of the main message.
//!     assert!(!res.main_failed());
//!
//!     // Returns bool which shows that there was panic during the execution
//!     // of the created messages during the main execution.
//!     //
//!     // Equals false if no others were called.
//!     assert!(!res.others_failed());
//!
//!     // Returns bool which shows that logs contain a given log.
//!     //
//!     // Syntax sugar around `res.log().iter().any(|v| v == arg)`.
//!     assert!(!res.contains(&Log::builder()));
//!
//!     // To build a log for assertion you need to use `Log` structure with its builders.
//!     // All fields here are optional.
//!     // Assertion with Logs from core are made on the Some(..) fields
//!     // You will run into panic if you try to set the already specified field.
//!     //
//!     // Constructor for success log.
//!     let _ = Log::builder();
//!
//!     // Constructor for error reply log.
//!     let _ = Log::error_builder();
//!
//!     // The first message to uploaded program is INIT message.
//!     //
//!     // Let’s send a new message after the program has been initialized.
//!     // The initialized program expects to receive a byte string "PING" and replies with a byte string "PONG".
//!     let res = ping_pong.send_bytes(100001, "PING");
//!
//!     // Other fields are set optionally by `dest()`, `source()`, `payload()`, `payload_bytes()`.
//!     //
//!     // The logic for `payload()` and `payload_bytes()` is the same as for `send()` and `send_bytes()`.
//!     // First requires an encodable struct. The second requires bytes.
//!     let log = Log::builder()
//!         .source(ping_pong_id)
//!         .dest(100001)
//!         .payload_bytes("PONG");
//!
//!     assert!(res.contains(&log));
//!
//!     let wrong_log = Log::builder().source(100001);
//!
//!     assert!(!res.contains(&wrong_log));
//!
//!     // Log also has `From` implementations from (ID, T) and from (ID_1, ID_2, T),
//!     // where ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>
//!     let x = Log::builder().dest(5).payload_bytes("A");
//!     let x_from: Log = (5, "A").into();
//!
//!     assert_eq!(x, x_from);
//!
//!     let y = Log::builder().dest(5).source(15).payload_bytes("A");
//!     let y_from: Log = (15, 5, "A").into();
//!
//!     assert_eq!(y, y_from);
//!
//!     assert!(!res.contains(&(ping_pong_id, ping_pong_id, "PONG")));
//!     assert!(res.contains(&(1, 100001, "PONG")));
//! ```
//! - Spending blocks:
//! ```ignore
//! // You may control time in the system by spending blocks.
//! //
//! // It adds the amount of blocks passed as arguments to the current block of the system.
//! // Same for the timestamp. Note, that for now 1 block in Gear-based network is 3 sec
//! // duration.
//! sys.spend_blocks(150);
//! ```
//! <!--
//! - Reading the program state:
//! ```ignore
//! // To read the program state you need to call one of two program's functions:
//! // `meta_state()` or `meta_state_with_bytes()`.
//! //
//! // The methods require the payload as the input argument.
//! //
//! // The first one requires payload to be CODEC Encodable, while the second requires payload
//! // implement `AsRef<[u8]>`, that means to be able to represent as bytes.
//! //
//! // Let we have the following contract state and `meta_state` function:
//! #[derive(Encode, Decode, TypeInfo)]
//! pub struct ContractState {
//!     a: u128,
//!     b: u128,
//! }
//!
//! pub enum State {
//!     A,
//!     B,
//! }
//!
//! pub enum StateReply {
//!     A(u128),
//!     B(u128),
//! }
//!
//! #[no_mangle]
//! unsafe extern "C" fn meta_state() -> *mut [i32; 2] {
//!     let query: State = msg::load().expect("Unable to decode `State`");
//!     let encoded = match query {
//!         State::A => StateReply::A(STATE.a),
//!         State::B => StateReply::B(STATE.b),
//!     }
//!     .encode();
//!     gstd::util::to_leak_ptr(encoded)
//! }
//!
//! // Let's send a query from gtest:
//! let reply: StateReply = self.meta_state(&State::A).expect("Meta_state failed");
//! let expected_reply = StateReply::A(10);
//! assert_eq!(reply, expected_reply);
//!
//! // If your `meta_state` function doesn't require input payloads,
//! // you can use `meta_state_empty` or `meta_state_empty_with_bytes` functions
//! // without any arguments.
//! ```
//! -->
//! - Balance:
//! ```ignore
//! // If you need to send a message with value you have to mint balance for the message sender:
//! let user_id = 42;
//! sys.mint_to(user_id, 5000);
//! assert_eq!(sys.balance_of(user_id), 5000);
//!
//! // To give the balance to the program you should use `mint` method:
//! let prog = Program::current(&sys);
//! prog.mint(1000);
//! assert_eq!(prog.balance(), 1000);
//! ```
#![deny(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

mod error;
mod log;
mod mailbox;
mod manager;
mod program;
mod system;

pub use crate::log::{CoreLog, Log, RunResult};
pub use codec;
pub use error::{Result, TestError};
pub use program::{calculate_program_id, Gas, Program, WasmProgram};
pub use system::System;

const EXISTENTIAL_DEPOSIT: u128 = 500;
const MAILBOX_THRESHOLD: u64 = 3000;
const WAITLIST_COST: u64 = 100;
const RESERVE_FOR: u32 = 1;
const RESERVATION_COST: u64 = 100;
const READ_COST: u64 = 20;
const WRITE_COST: u64 = 100;
const READ_PER_BYTE_COST: u64 = 10;
const WRITE_PER_BYTE_COST: u64 = 10;
const MODULE_INSTANTIATION_BYTE_COST: u64 = 20;
const MAX_RESERVATIONS: u64 = 256;
const EPOCH_DURATION_IN_BLOCKS: u32 = 600;
const INITIAL_RANDOM_SEED: u64 = 42;
const MODULE_INSTRUMENTATION_BYTE_COST: u64 = 13;
const MODULE_INSTRUMENTATION_COST: u64 = 297;
const DISPATCH_HOLD_COST: u64 = 200;
const RENT_COST: u128 = 330;
const VALUE_PER_GAS: u128 = 25;
