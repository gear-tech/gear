// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

use parity_scale_codec::Encode;

/// Message builder
pub struct Message {
    /// The maximum gas amount allowed to spend for the program
    /// creation and initialization;
    pub gas_limit: u64,
    /// Payload of this message.
    pub payload: Vec<u8>,
    /// Signer address
    ///
    /// TODO: introduce a better wrapper
    pub signer: [u8; 32],
    /// Value contains in this message.
    pub value: u128,
    /// The arbitrary data needed to generate an address for a new
    /// program (control of salt uniqueness is entirely on the function caller’s side);
    pub salt: Vec<u8>,
}

impl Message {
    /// New message from encodable type
    pub fn new<T: Encode>(payload: T) -> Self {
        Self {
            payload: payload.encode(),
            value: 0,
            signer: [0; 32],
            gas_limit: 0,
            salt: Default::default(),
        }
    }

    /// New message from raw bytes
    pub fn bytes(payload: impl AsRef<[u8]>) -> Self {
        Self {
            payload: payload.as_ref().into(),
            value: 0,
            signer: [0; 32],
            gas_limit: 0,
            salt: Default::default(),
        }
    }

    /// Set the value of this message
    pub fn value(mut self, value: u128) -> Self {
        self.value = value;
        self
    }

    /// NOTE: you don't need this method if you are simply
    /// sending messages.
    ///
    /// The arbitrary data needed to generate an address for a new program
    /// (control of salt uniqueness is entirely on the function caller’s side);
    pub fn salt(mut self, salt: Vec<u8>) -> Self {
        self.salt = salt;
        self
    }

    /// Set the signer of this message
    ///
    /// TODO: query the keypair from address
    pub fn signer(mut self, signer: impl Into<[u8; 32]>) -> Self {
        self.signer = signer.into();
        self
    }
}

impl<T: Encode> From<T> for Message {
    fn from(payload: T) -> Self {
        Self::new(payload)
    }
}
