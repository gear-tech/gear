// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
//! `gtest` is a library that provides a set of tools for testing Gear programs.
//! The most important structures are:
//!
//! - [`System`] — a structure that represents the environment of the Gear
//!   network. It contains the current block number, timestamp, and other
//!   parameters. It also stores the mailbox and the list of programs.
//! - [`Program`] — a structure that represents a Gear program. It contains the
//!   information about program and allows sending messages to other programs.
//! - [`Log`] — a structure that represents a message log. It allows checking
//!   the result of the program execution.
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
//! edition = "2024"
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
//! ```ignore
//! #![no_std]
//! use gstd::msg;
//!
//! #[unsafe(no_mangle)]
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
//! ```ignore
//! fn main() {
//!     gear_wasm_builder::build();
//! }
//! ```
//!
//! We will add a test that will check the program's behavior. To do this, we
//! will use the `gtest` library.
//!
//! Our test will consist of the following steps:
//!
//! 1. Initialize the `System` structure.
//! 2. Initialize the `Program` structure.
//! 3. Send an init message to the program. Even though we don't have the `init`
//!    function in our program, the first message to the program sent via
//!    `gtest` is always the init one.
//! 4. Send a handle message to the program.
//! 5. Check the result of the program execution.
//!
//! Add these lines to the bottom of the `lib.rs` file:
//!
//! ```no_run
//! #[cfg(test)]
//! mod tests {
//!     use gtest::{Log, Program, System};
//!
//!     // Alternatively, you can use the default users from `gtest::constants`:
//!     // `DEFAULT_USER_ALICE`, `DEFAULT_USER_BOB`, `DEFAULT_USER_CHARLIE`, `DEFAULT_USER_EVE`.
//!     // The full list of default users can be obtained with `gtest::constants::default_users_list`.
//!     const USER_ID: u64 = 100001;
//!
//!     #[test]
//!     fn test_ping_pong() {
//!         // Initialization of the common environment for running programs.
//!         let sys = System::new();
//!
//!         // Initialization of the current program structure.
//!         let prog = Program::current(&sys);
//!
//!         // Provide user with some balance.
//!         sys.mint_to(USER_ID, EXISTENTIAL_DEPOSIT * 1000);
//!
//!         // Send an init message to the program.
//!         let init_message_id = prog.send_bytes(USER_ID, b"Doesn't matter");
//!
//!         // Run execution of the block which will contain `init_message_id`
//!         let block_run_result = sys.run_next_block();
//!
//!         // Check whether the program was initialized successfully.
//!         assert!(block_run_result.succeed.contains(&init_message_id));
//!
//!         // Send a handle message to the program.
//!         let handle_message_id = prog.send_bytes(USER_ID, b"PING");
//!         let block_run_result = sys.run_next_block();
//!
//!         // Check the result of the program execution.
//!         // 1. Create a log pattern with the expected result.
//!         let log = Log::builder()
//!             .source(prog.id())
//!             .dest(USER_ID)
//!             .payload_bytes(b"PONG");
//!
//!         // 2. Check whether the program was executed successfully.
//!         assert!(block_run_result.succeed.contains(&handle_message_id));
//!
//!         // 3. Make sure the log entry is in the result.
//!         assert!(block_run_result.contains(&log));
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
//! ## Initialization of the network environment for running programs
//!
//! ```no_run
//! # use gtest::System;
//! let sys = System::new();
//! ```
//!
//! This emulates node's and chain's behavior. By default, the [`System::new`]
//! function sets the following parameters:
//!
//! - current block equals `0`
//! - current timestamp equals UNIX timestamp of your system
//! - starting message id equals `0x010000..`
//! - starting program id equals `0x010000..`
//!
//! ## Program initialization
//!
//! There are a few ways to initialize a program:
//!
//! - Initialize the current program using the [`Program::current`] function:
//!
//!     ```no_run
//!     # use gtest::Program;
//!     # let sys = gtest::System::new();
//!     let prog = Program::current(&sys);
//!     ```
//!
//! - Initialize a program from a Wasm-file with a default id using the
//!   [`Program::from_file`] function:
//!
//!     ```no_run
//!     # use gtest::Program;
//!     # let sys = gtest::System::new();
//!     let prog = Program::from_file(
//!         &sys,
//!         "./target/wasm32-gear/release/demo_ping.wasm",
//!     );
//!     ```
//!
//! - Initialize a program via builder:
//!
//!     ```no_run
//!     # use gtest::ProgramBuilder;
//!     # let sys = gtest::System::new();
//!     let prog = ProgramBuilder::from_file("your_gear_program.wasm")
//!         .with_id(105)
//!         .build(&sys);
//!     ```
//!
//!     Every place in this lib, where you need to specify some ids, it requires
//!   generic type `ID`, which implements ``Into<ProgramIdWrapper>``.
//!
//!     `ProgramIdWrapper` may be built from:
//!     - `u64`
//!     - `[u8; 32]`
//!     - `String`
//!     - `&str`
//!     - [`ActorId`](https://docs.gear.rs/gear_core/ids/struct.ActorId.html)
//!       (from `gear_core` one's, not from `gstd`).
//!
//!     `String` implementation means the input as hex (with or without "0x").
//!
//! ## Getting the program from the system
//!
//! If you initialize program not in this scope, in cycle, in other conditions,
//! where you didn't save the structure, you may get the object from the system
//! by id.
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! let prog = sys.get_program(105).unwrap();
//! ```
//!
//! ## Initialization of styled `tracing-subscriber`
//!
//! Initialization of styled `tracing-subscriber` to
//! print logs (only from gwasm` by default) into stdout:
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! sys.init_logger();
//! ```
//!
//! To specify printed logs, set the env variable `RUST_LOG`:
//!
//! ```bash
//! RUST_LOG="target_1=logging_level,target_2=logging_level" cargo test
//! ```
//!
//! ## Pre-requisites for sending a message
//!
//! Prior to sending a message, it is necessary to mint sufficient balance for
//! the sender to ensure coverage of the existential deposit and gas costs.
//!
//! ```no_run
//! # use gtest::constants::EXISTENTIAL_DEPOSIT;
//! # let sys = gtest::System::new();
//! let user_id = 42;
//! sys.mint_to(user_id, EXISTENTIAL_DEPOSIT * 1000);
//! ```
//!
//! Alternatively, you can use the default users from `gtest::constants`, which
//! have a preallocated balance, as the message sender.
//!
//! ```no_run
//! # use gtest::constants::DEFAULT_USERS_INITIAL_BALANCE;
//! # let sys = gtest::System::new();
//! assert_eq!(
//!     sys.balance_of(gtest::constants::DEFAULT_USER_ALICE),
//!     DEFAULT_USERS_INITIAL_BALANCE
//! );
//! ```
//!
//! ## Sending messages
//!
//! To send message to the program need to call one of two program's functions:
//!
//! - [`Program::send`] (or [`Program::send_with_value`] if you need to send a
//!   message with attached funds).
//! - [`Program::send_bytes`] (or [`Program::send_bytes_with_value`] if you need
//!   to send a message with attached funds).
//!
//! Both of the methods require sender id as the first argument and the payload
//! as second.
//!
//! The difference between them is pretty simple and similar to [`gstd`](https://docs.gear.rs/gstd/) functions [`msg::send`](https://docs.gear.rs/gstd/msg/fn.send.html) and [`msg::send_bytes`](https://docs.gear.rs/gstd/msg/fn.send_bytes.html).
//!
//! The first one requires payload to be CODEC Encodable, while the second
//! requires payload implement `AsRef<[u8]>`, that means to be able to represent
//! as bytes.
//!
//! [`Program::send`] uses [`Program::send_bytes`] under the hood with bytes
//! from `payload.encode()`.
//!
//! First message to the initialized program structure is always the init
//! message.
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! # let prog = gtest::Program::current(&sys);
//! let res = prog.send_bytes(100001, "INIT MESSAGE");
//! ```
//!
//! ## Processing the result of the program execution
//!
//! Any sending functions in the lib returns an id of the sent message.
//!
//! In order to actually get the result of the program execution the block
//! execution should be triggered (see "Block execution model" section).
//! Block execution function returns the result of the block run
//! ([`BlockRunResult`])
//!
//! It contains the final result of the processing message and others, which
//! were created during the execution.
//!
//! It has 2 main functions:
//!
//! - [`BlockRunResult::log`] — returns the reference to the Vec produced to
//!   users messages. You may assert them as you wish, iterating through them.
//! - [`BlockRunResult::contains`] — returns bool which shows that logs contain
//!   a given log. Syntax sugar around `res.log().iter().any(|v| v == arg)`.
//!
//! Fields of the type are public, and some of them can be really useful:
//!
//! - field `succeed` is a set of ids of messages that were successfully
//!   executed.
//! - field `failed` is a set of ids of messages that failed during the
//!   execution.
//!
//! To build a log for assertion you need to use [`Log`] structure with its
//! builders. All fields here are optional. Assertion with `Log`s from core are
//! made on the `Some(..)` fields. You will run into panic if you try to set the
//! already specified field.
//!
//! ```no_run
//! # use gtest::Log;
//! # use gear_core_errors::ErrorReplyReason;
//! // Constructor for success log.
//! let log = Log::builder();
//!
//! // Constructor for error reply log.
//! let log = Log::error_builder(ErrorReplyReason::RemovedFromWaitlist);
//! # let sys = gtest::System::new();
//! # let prog = gtest::Program::current(&sys);
//! // Other fields are set optionally by `dest()`, `source()`, `payload()`, `payload_bytes()`.
//! let log = Log::builder()
//!     .source(prog.id())
//!     .dest(100001)
//!     .payload_bytes("PONG");
//! ```
//!
//! Log also has `From` implementations from `(ID, T)` and from `(ID_1, ID_2,
//! T)`, where `ID: Into<ProgramIdWrapper>`, `T: AsRef<[u8]>`.
//!
//! ```no_run
//! # use gtest::Log;
//! let x = Log::builder().dest(5).payload_bytes("A");
//! let x_from: Log = (5, "A").into();
//! assert_eq!(x, x_from);
//!
//! let y = Log::builder().dest(5).source(15).payload_bytes("A");
//! let y_from: Log = (15, 5, "A").into();
//! assert_eq!(y, y_from);
//! ```
//!
//! ## Blocks execution model
//!
//! Block execution has 2 main step:
//! - tasks processing
//! - messages processing
//!
//! Tasks processing is a step, when all scheduled for the current block number
//! tasks are tried to be processed. This includes processing delayed
//! dispatches, waking waited messages and etc.
//!
//! Messages processing is a step, when messages from the queue are processed
//! until either the queue is empty or the block gas allowance is not enough for
//! the execution.
//!
//! Blocks can't be "spent" without their execution except for use the
//! [`System::run_scheduled_tasks`] method, which doesn't process the message
//! queue, but only processes scheduled tasks triggering blocks info
//! adjustments, which can be used to "spend" blocks.
//!
//! Note, that for now 1 block in Gear-based network is 3 sec duration.
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! // Spend 150 blocks by running only the task pool (7.5 mins for 3 sec block).
//! sys.run_scheduled_tasks(150);
//! ```
//!
//! Note that processing messages (e.g. by using
//! [`Program::send`]/[`Program::send_bytes`] methods) doesn't spend blocks, nor
//! changes the timestamp. If you write time dependent logic, you should spend
//! blocks manually.
//!
//! ## Balance
//!
//! There are certain invariants in `gtest` regarding user and program balances:
//!
//! * For a user to successfully send a message to the program, they must have
//!   sufficient balance to cover the existential deposit and gas costs.
//! * The program charges the existential deposit from the user upon receiving
//!   the initial message.
//!
//! As previously mentioned [here](#Pre-requisites-for-Sending-a-Message),
//! a balance for the user must be minted before sending a message. This balance
//! should be sufficient to cover the following: the user's existential deposit,
//! the existential deposit of the initialized program (the first message to the
//! program charges the program's existential deposit from the sender), and the
//! message's gas costs.
//!
//! The [`System::mint_to`] method can be utilized to allocate a balance to the
//! user or the program. The [`System::balance_of`] method may be used to verify
//! the current balance.
//!
//! ```no_run
//! # use gtest::Program;
//! # let sys = gtest::System::new();
//! // If you need to send a message with value you have to mint balance for the message sender:
//! let user_id = 42;
//! sys.mint_to(user_id, 5000);
//! assert_eq!(sys.balance_of(user_id), 5000);
//!
//! // To give the balance to the program you should use [`System::transfer`] method:
//! let mut prog = Program::current(&sys);
//! sys.transfer(user_id, prog.id(), 1000, true);
//! assert_eq!(prog.balance(), 1000);
//! ```
//!
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
//! // Let we have the following program state and `meta_state` function:
//! #[derive(Encode, Decode, TypeInfo)]
//! pub struct ProgramState {
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
//! #[unsafe(no_mangle)]
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
#![deny(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

mod error;
mod log;
mod manager;
mod program;
mod state;
mod system;

pub use crate::log::{BlockRunResult, CoreLog, Log};
pub use error::{Result, TestError};
pub use parity_scale_codec;
pub use program::{
    calculate_program_id, gbuild::ensure_gbuild, Gas, Program, ProgramBuilder, ProgramIdWrapper,
    WasmProgram,
};
pub use state::mailbox::ActorMailbox;
pub use system::System;

pub use constants::Value;
pub(crate) use constants::*;

/// Module containing constants of Gear protocol.
pub mod constants {
    /* Constant types */

    use gear_common::GasMultiplier;

    /// Numeric type representing value in Gear protocol.
    pub type Value = u128;

    /// Numeric type representing gas in Gear protocol.
    pub type Gas = u64;

    /// Numeric type representing blocks in Gear protocol.
    pub type Block = u32;

    /* Gas logic related constants */

    /// Gas allowance for executing user dispatch and set of generated
    /// by programs dispatches from execution of the user dispatch.
    pub const GAS_ALLOWANCE: Gas = 1_000_000_000_000;

    /// Max allowed user gas to apply for external message.
    pub const MAX_USER_GAS_LIMIT: Gas = 750_000_000_000;

    /// Gas multiplier used to calculate equivalence of gas in token value.
    pub const GAS_MULTIPLIER: GasMultiplier<Value, Gas> = GasMultiplier::ValuePerGas(VALUE_PER_GAS);

    /* Currency-related constants */

    /// Value per token.
    pub const UNITS: Value = 1_000_000_000_000;
    /// Minimal amount of value able to be sent. Defines accounts existence
    /// requirement.
    pub const EXISTENTIAL_DEPOSIT: Value = UNITS;
    /// Value per gas.
    pub const VALUE_PER_GAS: Value = 100;
    /// Duration of one block in msecs.
    pub const BLOCK_DURATION_IN_MSECS: u64 = 3000;
    /// Duration of one epoch.
    pub const EPOCH_DURATION_IN_BLOCKS: Block = 600;

    /* Storage-related constants */
    /// Extra amount of blocks must be reserved for storing in storage.
    pub const RESERVE_FOR: Block = 1;

    /* Execution-related constants */

    /// Maximal amount of reservations program may have.
    pub const MAX_RESERVATIONS: u64 = 256;
    /// Initial random seed for testing environment.
    pub const INITIAL_RANDOM_SEED: u64 = 42;

    /* Default users constants with initial balance */

    /// Default user id for Alice.
    pub const DEFAULT_USER_ALICE: u64 = u64::MAX - 1;
    /// Default user id for Bob.
    pub const DEFAULT_USER_BOB: u64 = u64::MAX - 2;
    /// Default user id for Charlie.
    pub const DEFAULT_USER_CHARLIE: u64 = u64::MAX - 3;
    /// Default user id for Eve.
    pub const DEFAULT_USER_EVE: u64 = u64::MAX - 4;

    /// Default list of users.
    pub const fn default_users_list() -> &'static [u64] {
        &[
            DEFAULT_USER_ALICE,
            DEFAULT_USER_BOB,
            DEFAULT_USER_CHARLIE,
            DEFAULT_USER_EVE,
        ]
    }

    /// Default initial balance for users.
    pub const DEFAULT_USERS_INITIAL_BALANCE: Value = 100_000 * UNITS;
}
