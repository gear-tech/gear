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

//! Core ROAST participant helpers.
//!
//! ```text
//! SignRequest -> NonceCommit -> SigningPackage -> PartialSignature
//! ```

mod participant;
mod tweak;

pub use participant::{ParticipantAction, ParticipantConfig, ParticipantEvent, RoastParticipant};
pub(crate) use tweak::{tweak_key_package, tweak_public_key_package};

#[cfg(test)]
mod tests;

use ethexe_common::{
    Address,
    crypto::{DkgSessionId, SignAggregate},
};
use gprimitives::H256;

/// ROAST signing session configuration.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// DKG session ID (era index)
    pub session: DkgSessionId,
    /// Message hash to sign
    pub msg_hash: H256,
    /// ActorId for key tweaking
    pub tweak_target: gprimitives::ActorId,
    /// Leader election attempt counter
    pub attempt: u32,
    /// Threshold (minimum signers)
    pub threshold: u16,
    /// All participants (sorted)
    pub participants: Vec<Address>,
    /// This participant's address
    pub self_address: Address,
}

/// Result of ROAST signing session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoastResult {
    /// Signing completed successfully
    Success(SignAggregate),
    /// Signing failed
    Failed(String),
}
