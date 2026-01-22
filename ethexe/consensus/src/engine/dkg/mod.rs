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

//! DKG (Distributed Key Generation) State Machine
//!
//! This module implements Pedersen VSS DKG protocol for validators.
//! The DKG process runs when a new validator set is elected for era+1.
//!
//! ## Protocol Phases:
//! 1. **Round1 (Commitment Phase)**: Each validator generates a secret polynomial
//!    and broadcasts Pedersen commitments to all other validators.
//! 2. **Round2 (Share Distribution)**: Each validator sends encrypted shares
//!    to other participants with proofs of correctness.
//! 3. **Verification & Culprits**: Validators verify received shares and report
//!    culprits if shares are invalid.
//! 4. **Completion**: If no culprits are detected, aggregate public key is computed.
//!
//! ## State Machine:
//! ```text
//! Idle
//!   |
//!   ├──> Round1Pending (waiting for commits)
//!   |       |
//!   |       └──> Round2Pending (waiting for shares)
//!   |              |
//!   |              └──> CulpritsPending (cheater detection)
//!   |                     |
//!   |                     └──> Completed (public key package ready)
//!   |
//!   └──> Failed (insufficient participants or protocol error)
//! ```

pub mod core;
pub mod engine;
pub mod storage;

pub use core::{DkgConfig, DkgProtocol, FinalizeResult};
pub use engine::{DkgEngine, DkgEngineEvent};
pub use storage::{DkgAction, DkgEvent, DkgManager, DkgState, DkgStateMachine};

use ethexe_common::{
    Address,
    crypto::{DkgKeyPackage, DkgPublicKeyPackage, DkgShare, DkgVssCommitment},
};

/// DKG session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Era index for this DKG session
    pub era_index: u64,
    /// List of validator addresses (sorted)
    pub validators: Vec<Address>,
    /// Threshold (minimum signers required)
    pub threshold: u16,
    /// This validator's address
    pub self_address: Address,
}

/// Result of DKG session
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DkgResult {
    /// DKG completed successfully
    Success(Box<DkgCompleted>),
    /// DKG failed (e.g., insufficient participants)
    Failed(String),
}

/// DKG completion payload (persisted by storage layer)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkgCompleted {
    pub public_key_package: DkgPublicKeyPackage,
    pub key_package: DkgKeyPackage,
    pub vss_commitment: DkgVssCommitment,
    pub share: DkgShare,
}
