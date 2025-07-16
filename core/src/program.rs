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

//! Module for programs.

use crate::{
    ids::{ActorId, MessageId},
    reservation::GasReservationMap,
};
use gprimitives::CodeId;
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

/// Program in different states in storage.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub enum Program<BlockNumber: Copy> {
    /// Program in active state.
    Active(ActiveProgram<BlockNumber>),
    /// Program has been exited (gr_exit was called)
    Exited(ActorId),
    /// Program has been terminated (`init` was failed)
    Terminated(ActorId),
}

impl<BlockNumber: Copy> Program<BlockNumber> {
    /// Returns whether the program is active.
    pub fn is_active(&self) -> bool {
        matches!(self, Program::Active(_))
    }

    /// Returns whether the program is exited.
    pub fn is_exited(&self) -> bool {
        matches!(self, Program::Exited(_))
    }

    /// Returns whether the program is terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Program::Terminated(_))
    }

    /// Returns whether the program is active and initialized.
    pub fn is_initialized(&self) -> bool {
        matches!(
            self,
            Program::Active(ActiveProgram {
                state: ProgramState::Initialized,
                ..
            })
        )
    }
}

/// Program is not an active one.
#[derive(Clone, Debug, derive_more::Display)]
#[display("Program is not an active one")]
pub struct InactiveProgramError;

impl<BlockNumber: Copy> core::convert::TryFrom<Program<BlockNumber>>
    for ActiveProgram<BlockNumber>
{
    type Error = InactiveProgramError;

    fn try_from(prog_with_status: Program<BlockNumber>) -> Result<Self, Self::Error> {
        match prog_with_status {
            Program::Active(p) => Ok(p),
            _ => Err(InactiveProgramError),
        }
    }
}

/// Active program in storage.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct ActiveProgram<BlockNumber: Copy> {
    /// Continuous intervals amount in program allocations.
    pub allocations_tree_len: u32,
    /// Infix of memory pages storage (is used for memory wake after pausing)
    pub memory_infix: MemoryInfix,
    /// Gas reservation map.
    pub gas_reservation_map: GasReservationMap,
    /// Code id of the program.
    pub code_id: CodeId,
    /// Initialization state of the program.
    pub state: ProgramState,
    /// Block number when the program will be expired.
    pub expiration_block: BlockNumber,
}

/// Enumeration contains variants for program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub enum ProgramState {
    /// `init` method of a program has not yet finished its execution so
    /// the program is not considered as initialized.
    Uninitialized {
        /// identifier of the initialization message.
        message_id: MessageId,
    },
    /// Program has been successfully initialized and can process messages.
    Initialized,
}

/// Struct defines infix of memory pages storage.
#[derive(Clone, Copy, Debug, Default, Decode, Encode, PartialEq, Eq, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryInfix(u32);

impl MemoryInfix {
    /// Constructing function from u32 number.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return inner u32 value.
    pub fn inner(&self) -> u32 {
        self.0
    }
}
