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

#![no_std]

use codec::{Decode, Encode};
use gstd::ActorId;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    include! {"./code.rs"}
}

/// Sending request.
#[derive(Encode, Decode, Debug, Clone)]
pub struct SendingRequest {
    /// Account id for sending value.
    pub account_id: ActorId,
    /// Gas limit to be sent.
    pub gas_limit: Option<u64>,
    /// Amount of value to be sent.
    pub value: u128,
}

impl SendingRequest {
    /// Create request without gas limit.
    pub fn gasless(account_id: impl Into<ActorId>, value: u128) -> Self {
        Self {
            account_id: account_id.into(),
            gas_limit: None,
            value,
        }
    }

    /// Create request with explicit gas limit.
    pub fn gasful(account_id: impl Into<ActorId>, gas_limit: u64, value: u128) -> Self {
        Self {
            account_id: account_id.into(),
            gas_limit: Some(gas_limit),
            value,
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct TestData {
    // For request data.
    pub gas_limit: Option<u64>,
    pub value: u128,
    // Extra data (especially for gasless sending).
    pub gas_limit_to_send: u64,
    pub extra_gas: u64,
}

impl TestData {
    pub fn gasless(value: u128, mailbox_threshold: u64) -> Self {
        Self {
            gas_limit: None,
            value,
            gas_limit_to_send: mailbox_threshold,
            extra_gas: mailbox_threshold * 5,
        }
    }

    pub fn gasful(gas_limit: u64, value: u128) -> Self {
        Self {
            gas_limit: Some(gas_limit),
            value,
            gas_limit_to_send: gas_limit,
            extra_gas: 0,
        }
    }

    pub fn request(&self, account_id: impl Into<ActorId>) -> SendingRequest {
        if let Some(gas_limit) = self.gas_limit {
            SendingRequest::gasful(account_id, gas_limit, self.value)
        } else {
            SendingRequest::gasless(account_id, self.value)
        }
    }
}
