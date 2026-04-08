// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Vara.eth node wrapper.
//!
//! Node wraper is a wrapper around the Vara.eth node.
//! Internally, it do the next things:
//! - spawns the Vara.eth node process
//!     - spanws the Anvil node process
//!     - deploy Ethereum smart-contracts on Anvil
//! - provides the acess to node PRC endpoints and Ethereum RPC endpoint
//!
//! ## Modules
//! - [`node`] - provides the [VaraEth] struct - the node configurator
//! - [`instance`] - provides the [VaraEthInstance] struct - the instance which holds the inner spanwed process and RPC endpoints.
//! - [`error`] - provides the [Error] enum - the error type for module errors.

#![warn(missing_docs, unreachable_pub)]

/// Crate errors module.
pub mod error;
pub use error::Error;

/// The node wrapper spawned instance module
pub mod instance;
pub use instance::VaraEthInstance;

/// The node wrapper configuration module.
pub mod node;
pub use node::VaraEth;
