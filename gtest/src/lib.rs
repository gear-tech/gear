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
//! `gtest` is excellent for unit and integration testing.
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
//!   parameters. It also stores the mailbox, the list of programs, manages
//!   the state and does many other useful things.
//! - [`Program`] — a structure that represents a Gear program. It contains the
//!   information about program and allows sending messages to it.
//! - [`BlockRunResult`] - a structure that represents the result of running a block. It
//!   contains the status of the block execution, the list of events, and other
//!   relevant information.
//! - [`UserMessageEvent`] — a structure that represents a message addressed to user, but
//!   not reached the mailbox, so it's stored as an event. Most of times, there's no need
//!   to use it directly, but [`EventBuilder`] is used instead.
//!
//! Let's take a closer look at how to write tests using `gtest`.
//!
//! ### Import `gtest` lib
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
//! Make sure you use the latest version of the `gtest` and other crates for program development.
//!
//! ### Program example
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
//! will use the `gtest` library. Add these lines to the bottom of the `lib.rs` file:
//!
//! ```no_run
//! #[cfg(test)]
//! mod tests {
//!     use gtest::{EventBuilder, Program, System};
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
//!         // The `Program` type automatically detects the kind of the message,
//!         // if it's program initialization message or a handle message.
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
//!         // 1. Using `EventBuilder` build an expected to be found event.
//!         let event = EventBuilder::new()
//!             .source(prog.id())
//!             .dest(USER_ID)
//!             .payload_bytes(b"PONG")
//!             .build();
//!
//!         // 2. Check whether the program was executed successfully.
//!         assert!(block_run_result.succeed.contains(&handle_message_id));
//!
//!         // 3. Make sure the event is in the result.
//!         assert!(block_run_result.contains(&event));
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
//! ## [`System`]
//!
//! The [`System`] represents a complete Gear blockchain environment in a testing context. It maintains:
//! - Block height and timestamp progression
//! - Message queue and dispatch processing
//! - Program storage and execution state
//! - User and program balances
//! - Mailbox for user messages
//! - Task scheduling and execution
//!
//! ### `System` is a per-thread singleton.
//! The `System` is implemented as a per-thread singleton, meaning only one instance can exist
//! per thread at any given time. Attempting to create multiple instances will result in a panic:
//!
//! ```should_panic
//! # use gtest::System;
//! let sys1 = System::new(); // OK
//! let sys2 = System::new(); // Panics: "Impossible to have multiple instances of the `System`."
//! ```
//!
//! This design ensures consistent state management for globally accessed storages, managed by `System`.
//!
//! ### Logging configuration
//! The `System` provides several methods for configuring logging output during tests (see [`System::init_logger`] and other methods).
//! These methods allow to pre-set logging targets and levels for convenience, or control logging via the `RUST_LOG` environment variable.
//!
//! ### Blockchain state advancement methods
//! The `System` provides several methods for advancing blockchain state through block execution. These methods are:
//! - [`System::run_next_block`]
//! - [`System::run_next_block_with_allowance`]
//! - [`System::run_to_block`]
//! - [`System::run_scheduled_tasks`]
//!
//! What's common between them is that they all move forward the block height and timestamp, possibly executing
//! two main messages storages - queue and task pool. Another commonality is that they all return a result of type [`BlockRunResult`],
//! which is going to be covered later.
//!
//! ### Balance management
//! The `System` provides methods for managing user and program balances:
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! # use gtest::constants::EXISTENTIAL_DEPOSIT;
//! let user_id = 42;
//! let program_id = 100;
//!
//! // Mint balance to a user (required before sending messages)
//! sys.mint_to(user_id, EXISTENTIAL_DEPOSIT * 1000);
//!
//! // Transfer value to the program directly
//! sys.transfer(user_id, program_id, 5000, true);
//!
//! // Check balance
//! let balance = sys.balance_of(user_id);
//! println!("User balance: {}", balance);
//! ```
//!
//! Prior to sending a message, it is necessary to mint sufficient balance for the sender
//! to ensure coverage of the existential deposit and gas costs. Alternatively, as a sender
//! you can use the default users with preallocated balance from [`constants`] module.
//!
//! ### Code storage
//! Every program in `gtest` is basically a WASM binary and it's state. Both state and binary are stored and managed inside the `System`.
//! The `System` allows storing codes using various code submission methods. For example:
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! # use std::fs;
//! // Submit WASM bytes code
//! let code = fs::read("demo.wasm").unwrap();
//! let code_id = sys.submit_code(code);
//!
//! // Retrieve previously submitted code
//! let original_code = sys.submitted_code(code_id);
//! ```
//!
//! For more methods see `System` methods documentation.
//!
//! ### Programs API
//! The `System` provides methods to access all known programs. A known program is
//! at least once instantiated within the `System` program of [`Program`] type. [`System::get_program`] - allows retrieving
//! an instance of the `Program` by id. To get all known programs a [`System::programs`] method must be called.
//!
//! Although `Program`s are going to be covered later, it must be stated, that most of times a desired `Program`
//! is instantiated directly by `Program` methods. The `System` methods can be used when instance of the known
//! program was lost or more than one instance of the same program is required.
//!
//! ### User mailbox
//! Messages sent from program to user can end up being stored in user's mailbox. To access all mailbox messages
//! of the particular user, the [`System::get_mailbox`] method must be used. It instantiates user's mailbox
//! managing interface - [`UserMailbox`], which is going to be covered later. The method is an only mean by which
//! a `UserMailbox` can be instantiated.
//!
//! ### No message calculation
//! Sometimes it's useful to calculate the result of the message execution without actually sending message.
//! That could be a case when message produces unwanted side effects on each received message, which can't be
//! easily reverted. That's where the [`System::calculate_reply_for_handle`] method comes in handy. It forms
//! a message with desired parameters and calculates what reply the destination program would send.
//!
//! ## [`Program`]
//! The `Program` type is a representation of a Gear program, which gives an interface for interacting with it
//! and reading its state.
//!
//! ### `Program` instantiation
//! There are several ways to instantiate a `Program`:
//! - [`Program::current`] - instantiates current crate's program. The method recognizes a path
//!   that stores the compiled WASM binary of the current crate program.
//! - [`Program::from_file`] - instantiates a program from a specified WASM file path.
//! - [`Program::from_binary_with_id`] - instantiates a program from given WASM binary.
//!
//! All of these methods use under the hood the [`ProgramBuilder`], which itself handles instantiation
//! job and provides additional configuration options, like defining custom program id.
//!
//! ### Sending messages
//! The `Program` provides methods for sending messages to the program:
//! - [`Program::send`] (or [`Program::send_with_value`] if you need to send a
//!   message with attached funds).
//! - [`Program::send_bytes`] (or [`Program::send_bytes_with_value`] if you need
//!   to send a message with attached funds).
//!
//! Both of the methods require sender id as the first argument and the payload
//! as second.
//!
//! The difference between them is pretty simple and similar to [`gstd`](https://docs.gear.rs/gstd/) functions
//! [`msg::send`](https://docs.gear.rs/gstd/msg/fn.send.html) and [`msg::send_bytes`](https://docs.gear.rs/gstd/msg/fn.send_bytes.html).
//!
//! The first one requires payload to be parity-scale-codec encodable, while the second
//! requires payload implement `AsRef<[u8]>`, that means to be able to represent
//! as bytes. Should be noted, that parity-scale-codec is re-exported by `gtest`.
//!
//! [`Program::send`] uses [`Program::send_bytes`] under the hood with bytes
//! from `payload.encode()`.
//!
//! Under the hood the `Program` detects what dispatch kind should have a message to the program.
//! If it's a first message, it instantiates from provided data *init* dispatch kind. Otherwise, it's
//! going to be a *handle* message.
//!
//! ```no_run
//! # use gtest::{System, Program};
//! let sys = System::new();
//! let prog = Program::current(&sys);
//! let init_message_id = prog.send_bytes(100001, "INIT MESSAGE");
//! let handle_message_id = prog.send(100001, "HANDLE MESSAGE");
//! ```
//!
//! ### `Program` info
//! The `Program` provides methods to access its info:
//! - [`Program::id`] - returns program id.
//! - [`Program::balance`] - returns program balance.
//! - [`Program::read_state_bytes`] and [`Program::read_state`] - returns program state.
//!
//! The latter two methods actually require a WASM program to define extern `state` function,
//! which will be considered as an entry-point to the program when the latter methods are called.
//!
//! Besides, `Program` provides methods to save program's memory state to a file or to load state
//! from the file and update the program's state in the `System`. This is useful when one needs
//! to have a program with a specific state, like having two programs with exact same state.
//!
//! ### Program id
//! The `gtest` introduces a new abstraction over classical program id of [`ActorId`](https://docs.rs/gear-core/latest/gear_core/ids/struct.ActorId.html) type.
//! This abstraction is [`ProgramIdWrapper`]. It's main purpose to ease use of `gtest` API in a way that every method, which requires a user or program id as
//! an argument, actually accepts a trait bound `Into<ProgramIdWrapper>`. The best part of it, is that `ProgramIdWrapper` can be constructed from several types:
//! - `u64`;
//! - `[u8; 32]`;
//! - `String` (hex representation, with or without "0x");
//! - `&str` (hex representation, with or without "0x");
//! - `ActorId` (from `gear_core` one's, not from `gstd`).
//! - `Vec<u8>` and slice of `u8` (`&[u8]`).
//!
//! So when a method transforms an argument into `ProgramIdWrapper`, the latter is easily transformed then into
//! `ActorId`.
//!
//! ### __Mock__ programs
//! An exclusive feature of the `gtest` is an ability to create mock programs, which doesn't have a binary code,
//! or a persistent storage. Mock program is the one implementing [`WasmProgram`] trait, which defines how init
//! and handle messages must be handled by the program. Let's look at the example:
//!
//! ```no_run
//! # use gtest::WasmProgram;
//!
//! // Global state, because mutation of the `MockCounter` fields won't be saved.
//! static mut COUNTER: u32 = 0;
//!
//! #[derive(Debug)]
//! struct MockCounter;
//!
//! impl WasmProgram for MockCounter {
//!     // Handles init messages.
//!     //
//!     // Sets as initial counter state a value encoded in init message payload.
//!     // In case received bytes are not `u32` bytes, returns error, which will be
//!     // processed as a message execution trap, i.e. will result in error reply.
//!     //
//!     // In case of success returns `None`, which means no custom reply will be sent,
//!     // just successful auto-reply.
//!     fn init(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
//!         unsafe {
//!             COUNTER = u32::from_le_bytes(
//!                 payload
//!                     .try_into()
//!                     .map_err(|_| "Init payload is not u32 in bytes")?,
//!             );
//!         }
//!         
//!         Ok(None)
//!     }
//!
//!     // Handles handle messages.
//!     //
//!     // In case of receiving "INC" bytes, increments internal counter state.
//!     // In case of receiving "GET" bytes, returns current counter state in bytes
//!     // as a reply payload.
//!     //
//!     // All other payloads are treated as unknown and result in error reply.
//!     fn handle(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
//!         unsafe {
//!             match payload.as_slice() {
//!                 b"INC" => {
//!                     COUNTER += 1;
//!                     Ok(None)
//!                 }
//!                 b"GET" => Ok(Some(COUNTER.to_le_bytes().to_vec())),
//!                 _ => Err("Unknown handle command"),
//!             }
//!         }
//!     }
//!
//!     // Mock programs are stored in `gtest` storage as trait objects
//!     // hidden under the `Box` pointer. Due to internal implementation
//!     // the program object must be cloneable. So this method
//!     // provides a way to clone the boxed trait object.
//!     fn clone_boxed(&self) -> Box<dyn WasmProgram> {
//!         Box::new(MockCounter)
//!     }
//!
//!     // Handles reading the program state requests.
//!     //
//!     // This emulates calling `state` extern function possibly
//!     // defined in the WASM program.
//!     fn state(&mut self) -> Result<Vec<u8>, &'static str> {
//!         unsafe { Ok(COUNTER.to_le_bytes().to_vec()) }
//!     }
//! }
//!
//! #[test]
//! fn test_mock() {
//!     use gtest::{System, Program, EventBuilder};
//!     use gtest::constants::DEFAULT_USER_ALICE;
//!
//!     let sys = System::new();
//!
//!     // Instantiate the mock program with `Program` interface
//!     let program = Program::mock(&sys, MockCounter);
//!
//!     // Initialize the program with `42` as initial counter value
//!     let init_mid = program.send_bytes(DEFAULT_USER_ALICE, 42u32.to_le_bytes());
//!     let res = sys.run_next_block();
//!     assert!(res.succeed.contains(&init_mid));
//!
//!     // Check the counter
//!     let handle_mid = program.send_bytes(DEFAULT_USER_ALICE, b"GET");
//!     let res = sys.run_next_block();
//!     assert!(res.contains(&EventBuilder::new()
//!         .with_source(program.id())
//!         .with_destination(DEFAULT_USER_ALICE)
//!         .with_payload_bytes(42u32.to_le_bytes())
//!         .with_reply_to(handle_mid)
//!     ));
//!
//!     // Increment the counter
//!     let handle_mid = program.send_bytes(DEFAULT_USER_ALICE, b"INC");
//!     let res = sys.run_next_block();
//!     assert!(res.succeed.contains(&handle_mid));
//!
//!     // Check the counter again
//!     let handle_mid = program.send_bytes(DEFAULT_USER_ALICE, b"GET");
//!     let res = sys.run_next_block();
//!     assert!(res.contains(&EventBuilder::new()
//!         .with_source(program.id())
//!         .with_destination(DEFAULT_USER_ALICE)
//!         .with_payload_bytes(43u32.to_le_bytes())
//!         .with_reply_to(handle_mid)
//!     ));
//! }
//! ```
//!
//! Mock programs are instantiated with the [`Program::mock`] and [`Program::mock_with_id`] methods.
//! When executed there's no actual WASM executor is created, but, instead, the methods of the trait
//! are called directly. Because of the fact that mock programs doesn't have any persistent state,
//! if you want to have some, it can be done with static variables as shown in the example above.
//!
//! ## [`BlockRunResult`]
//! After calling message sending methods, messages are stored in the `System` message queue.
//! To actually process the messages, one of the block execution methods must be called. They were
//! mentioned earlier:
//! - [`System::run_next_block`] - runs one block, processing as many messages as possible.
//! - [`System::run_next_block_with_allowance`] - runs one block, processing as many messages as possible,
//!   while gas allowance for the block is limited by the provided value. The first method uses a default
//!   allowance - [`constants::GAS_ALLOWANCE`].
//! - [`System::run_to_block`] - runs blocks until the specified block number is reached.
//!
//! All of these methods actually pop messages from the queue and process them, mutating then storages
//! which were requested to be mutated by those messages, i.e. programs state, balance and etc. Programs
//! can also request not immediate, scheduled actions (tasks), which are stored in the `System` task pool.
//! So the block execution model of those methods is a two step model:
//! - tasks processing
//! - messages processing
//!
//! Tasks processing is a step, when all scheduled for the current block number
//! tasks are tried to be processed. This includes processing delayed
//! dispatches, waking waited messages and etc.
//!
//! Blocks can't be "spent" without their execution except for use the
//! [`System::run_scheduled_tasks`] method, which doesn't process the message
//! queue, but only processes scheduled tasks triggering blocks info (height and timestamp)
//! adjustments, which can be treated as "spending" blocks, if no tasks are scheduled.
//! Note, that for now 1 block in Gear-based network is 3 sec duration.
//!
//! ```no_run
//! # let sys = gtest::System::new();
//! // Spend 150 blocks by running only the task pool (7.5 mins for 3 sec block).
//! sys.run_scheduled_tasks(150);
//! ```
//!
//! All of methods mentioned above return a result of type `BlockRunResult`, which
//! encapsulates the outcome of the block execution, including any events that were emitted
//! during the block run, set of messages that were successfully executed, those that failed,
//! also those that were skipped. The type also gives information about the block itself, like
//! its height and timestamp, gas allowance spent, total messages processed.
//!
//! ## [`UserMessageEvent`]
//! Messages sent from program to user can end up being stored in user's mailbox. But if conditions
//! for storing the message in mailbox are not met, the message is stored in a collection of events
//! of `UserMessageEvent` type. The `UserMessageEvent` has basically same fields as a regular message
//! along with methods accessing them. Besides, it has [`UserMessageEvent::decode_payload`] method,
//! which can be used to decode the payload bytes into parity-scale-codec decodable type.
//!
//! The collection of events can be accessed from `BlockRunResult`. Here's a raw example of that:
//! ```no_run
//! // Access events collection and iterate through it.
//! let events = block_run_result.events();
//! assert!(events.iter().any(|event| event.id() == target_event_id));
//! ```
//!
//! ## [`EventBuilder`]
//! The event of `UserMessageEvent` type can't be constructed manually. The `gtest` provides
//! a builder for that - [`EventBuilder`]. What's more, most of API which expects an event
//! as an argument, actually waits for a type matching `impl Into<UserMessageEvent>` trait bound.
//! The `EventBuilder` itself can be built into `UserMessageEvent` with its [`EventBuilder::build`] method,
//! or converted into the event type, because it implements `Into<UserMessageEvent>`. So `EventBuilder`
//! can be used directly as an argument for such methods.
//!
//! So the last example with accessing events in `BlockRunResult` can be rewritten like this:
//! ```no_run
//! # use gtest::EventBuilder;
//! // Check whether the collection contains a target event using a convenience method of `BlockRunResult`.
//! assert!(block_run_result.contains(&EventBuilder::new()
//!     .with_source(program.id())
//!     .with_destination(USER_ID)
//!     .with_payload_bytes(b"PONG")
//! ));
//!
//! // Or with iterating through the collection.
//! let target_event = EventBuilder::new()
//!     .with_source(program.id())
//!     .with_destination(USER_ID)
//!     .with_payload_bytes(b"PONG")
//!     .build();
//! assert!(block_run_result.events().iter().any(|event| event == &target_event));
//! ```
//!
//! ## [`UserMailbox`]
//! The definition of the `UserMailbox` was given earlier - it's a managing interface for a particular
//! user mailbox. The interface can be used to:
//! - check if some message is in the mailbox ([`UserMailbox::contains`] method);
//! - reply to messages in the mailbox ([`UserMailbox::reply`] or [`UserMailbox::reply_bytes`] method);
//! - claim value inside the message without replying on it ([`UserMailbox::claim_value`] method);
//!
//! There was given an example of how to find an event in the `BlockRunResult` events collection. The same
//! `EventBuilder` can be used to build a message to be searched in the mailbox, because `UserMessageEvent`
//! can be compared to a mailbox message. Here's an example:
//! ```no_run
//! # use gtest::{System, Program, EventBuilder};
//! # use gtest::constants::DEFAULT_USER_ALICE;
//! let sys = System::new();
//! let prog = Program::current(&sys);
//!
//! // Say, the program sends a message to user mailbox
//! let mid = program.send_bytes(DEFAULT_USER_ALICE, b"PING");
//! let res = sys.run_next_block();
//! assert!(res.succeed.contains(&mid));
//!
//! let alice_mailbox = sys.get_mailbox(DEFAULT_USER_ALICE);
//! assert!(alice_mailbox.contains(&EventBuilder::new()
//!     .with_source(prog.id())
//!     .with_destination(DEFAULT_USER_ALICE)
//!     .with_payload_bytes(b"PONG")
//! ));
//! ```
//!
//! ## Builtins
//! Gear network has some built-in programs, which provide specific functionality.
//! The `gtest` provides their ids and request/response types for interaction with them.
//! The built-in programs are:
//! - [BLS12-381](https://wiki.vara.network/docs/build/builtinactors/bia-bls)
//! - [Ethereum Bridge](https://wiki.vara.network/docs/build/builtinactors/bia-bridge)
//!
//! For the BLS12-381 actor the provided items are:
//! - [`BLS12_381_ID`] - id, that can be used to send messages to the BLS12-381 built-in actor.
//! - [`Bls12_381Request`] - enum of requests that can be sent to the BLS12-381 built-in actor.
//! - [`Bls12_381Response`] - enum of responses that can be received from the BLS12-381 built-in actor.
//!
//! For the Ethereum Bridge actor the provided items are the same:
//! - [`ETH_BRIDGE_ID`] - id.
//! - [`EthBridgeRequest`] - enum of requests.
//! - [`EthBridgeResponse`] - enum of responses.
//!
//! These request and response types can be used with `Program` methods for sending messages
//! or receiving replies/mailbox messages/events. Although these types are useful for direct
//! interaction with built-in actors or decoding replies from them by users, most of times,
//! the interaction with built-in actors is done by programs.

#![deny(missing_docs)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

mod artifacts;
mod builtins;
mod error;
mod manager;
mod program;
mod state;
mod system;

pub use crate::artifacts::{BlockRunResult, EventBuilder, UserMessageEvent};
pub use builtins::{
    BLS12_381_ID, Bls12_381Request, Bls12_381Response, ETH_BRIDGE_ID, EthBridgeRequest,
    EthBridgeResponse,
};
pub use error::{Result, TestError};
pub use parity_scale_codec;
pub use program::{
    Program, ProgramBuilder, ProgramIdWrapper, WasmProgram, calculate_program_id,
    gbuild::ensure_gbuild,
};
pub use state::mailbox::UserMailbox;
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
    pub type BlockNumber = u32;

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
    pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 600;

    /* Storage-related constants */
    /// Extra amount of blocks must be reserved for storing in storage.
    pub const RESERVE_FOR: BlockNumber = 1;

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
