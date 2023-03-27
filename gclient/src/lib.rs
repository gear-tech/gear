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

//! Utility library for writing end-to-end tests for Gear programs.
//!
//! This crate can be considered a companion of the
//! [`gtest`](https://docs.gear.rs/gtest/) when covering the program code
//! with tests. When `gtest` is most appropriate for unit and integration
//! tests, `gclient` fits better for higher-level debugging.
//!
//! `gclient` is intended to test Gear programs with a real blockchain network.
//! It allows you to send extrinsics and RPCs by connecting to the network.
//!
//! It is essential to underline that testing with `gclient` requires the
//! running node as the second part of the test suite. The `gclient` interacts
//! with the node over the WebSocket protocol. Depending on the purpose of
//! testing, `gclient` can communicate with either a local or a remote node. The
//! best choice is to use the **local node in developer mode** for initial
//! debugging and continuous integration.
//!
//! Testing with `gclient` is slower than `gtest` and produces more build
//! artifacts, so it is better suited as the last mile in quality control.
//! However, `gclient` gives the most accurate test results.
//!
//! # Usage
//!
//! To use the `gclient` library, you must import it into your `Cargo.toml` file
//! in the `[dev-dependencies]` block. Also, you need to add some external
//! crates that are used together with `gclient`:
//!
//! ```toml
//! # ...
//!
//! [dev-dependencies]
//! gclient = { git = "https://github.com/gear-tech/gear.git" }
//! tokio = { version = "1.23.0", features = ["full"] }
//!
//! [patch.crates-io]
//! sp-core = { git = "https://github.com/gear-tech/substrate.git", branch = "gear-stable" }
//! sp-runtime = { git = "https://github.com/gear-tech/substrate.git", branch = "gear-stable" }
//! ```
//!
//! Download the latest node binary for your operating system
//! from <https://get.gear.rs>. Then unpack the package and run the node. Here
//! we assume the node is running in developer mode:
//!
//! ```shell
//! ./gear --dev
//! ```
//!
//! The final step is to write tests in a separate `tests` directory and make
//! `cargo` to execute them:
//!
//! ```shell
//! cargo test
//! ```
//!
//! # Examples
//!
//! Simple test example that uploads the program and sends the `PING` message.
//!
//! ```
//! use gclient::{EventProcessor, GearApi, Result};
//!
//! const WASM_PATH: &str = "./target/wasm32-unknown-unknown/release/first_gear_app.opt.wasm";
//!
//! #[tokio::test]
//! async fn test_example() -> Result<()> {
//!     // Create API instance
//!     let api = GearApi::dev().await?;
//!
//!     // Subscribe to events
//!     let mut listener = api.subscribe().await?;
//!
//!     // Check that blocks are still running
//!     assert!(listener.blocks_running().await?);
//!
//!     // Calculate gas amount needed for initialization
//!     let gas_info = api
//!         .calculate_upload_gas(None, gclient::code_from_os(WASM_PATH)?, vec![], 0, true)
//!         .await?;
//!
//!     // Upload and init the program
//!     let (message_id, program_id, _hash) = api
//!         .upload_program_bytes_by_path(
//!             WASM_PATH,
//!             gclient::now_micros().to_le_bytes(),
//!             vec![],
//!             gas_info.min_limit,
//!             0,
//!         )
//!         .await?;
//!
//!     assert!(listener.message_processed(message_id).await?.succeed());
//!
//!     let payload = b"PING".to_vec();
//!
//!     // Calculate gas amount needed for handling the message
//!     let gas_info = api
//!         .calculate_handle_gas(None, program_id, payload.clone(), 0, true)
//!         .await?;
//!
//!     // Send the PING message
//!     let (message_id, _hash) = api
//!         .send_message_bytes(program_id, payload, gas_info.min_limit, 0)
//!         .await?;
//!
//!     assert!(listener.message_processed(message_id).await?.succeed());
//!
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

mod api;
mod node;
mod utils;

pub use api::{calls::*, error::*, listener::*, GearApi};
pub use node::ws::WSAddress;
pub use utils::*;
