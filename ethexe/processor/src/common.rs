// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use ethexe_common::gear::StateTransition;
use gprimitives::CodeId;
use parity_scale_codec::{Decode, Encode};

/// Local changes that can be committed to the network or local signer.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum LocalOutcome {
    /// Produced when code with specific id is recorded and validated.
    CodeValidated {
        id: CodeId,
        valid: bool,
    },

    Transition(StateTransition),
}

pub fn unpack_i64(packed: i64) -> (i32, i32) {
    let high = (packed >> 32) as i32; // Shift right and cast
    let low = (packed & 0xFFFFFFFF) as i32; // Mask and cast
    (high, low)
}

pub fn pack_i64(high: i32, low: i32) -> i64 {
    ((high as i64) << 32) | (low as i64 & 0xFFFFFFFF)
}
