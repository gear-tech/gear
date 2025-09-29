// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use gearexe_common::gear::StateTransition;
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
