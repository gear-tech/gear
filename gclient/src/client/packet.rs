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
    /// Payload of this message.
    pub payload: Vec<u8>,
    /// Value contains in this message.
    pub value: u128,
}

impl Message {
    /// New message from encodable type
    pub fn new<T: Encode>(payload: T) -> Self {
        Self {
            payload: payload.encode(),
            value: 0,
        }
    }

    /// New message from raw bytes
    pub fn bytes(payload: impl AsRef<u8>) -> Self {
        Self {
            payload: payload.as_ref().into(),
            value: 0,
        }
    }

    /// Set the value of this message
    pub fn value(mut self, value: u128) -> Self {
        self.value = value;
        self
    }
}

impl<T> From<T: Encode> for Message {
    fn from(payload: T) -> Self {
        Self::new(payload)
    }
}
