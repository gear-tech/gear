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

//! ROAST (Robust Asynchronous Schnorr Threshold) Signing
//!
//! This module implements the ROAST protocol for threshold signatures using FROST.
//! ROAST provides robustness against Byzantine participants and network failures.
//!
//! ## Protocol Overview:
//!
//! 1. **Leader Election**: Deterministic leader selection based on validators, msg_hash, and era
//! 2. **Nonce Commitment Phase**: Participants send nonce commitments to the leader
//! 3. **Partial Signature Phase**: Leader aggregates nonces and participants send partial signatures
//! 4. **Aggregation**: Leader combines partial signatures into final 96-byte signature
//! 5. **Failover**: If leader fails, elect next leader and retry
//!
//! ## Key Features:
//!
//! - **Byzantine Fault Tolerance**: Works even if up to (n - threshold) participants fail
//! - **Asynchronous**: No global synchronization required
//! - **Deterministic Leaders**: Leader election is deterministic and verifiable
//! - **Key Tweaking**: Support for ActorId-specific signing
//!
//! ## State Machine:
//!
//! ```text
//! Idle
//!   |
//!   ├──> WaitingForNonces (leader collects nonce commitments)
//!   |       |
//!   |       └──> WaitingForPartials (leader collects partial signatures)
//!   |              |
//!   |              └──> Completed (final signature ready)
//!   |
//!   └──> Failed (timeout or insufficient participants)
//! ```
//!
//! ## Message Flow (ASCII):
//!
//! ```text
//! Leader                    Participants
//!   |  SignSessionRequest  ->  |
//!   |  <- SignNonceCommit      |
//!   |  SignNoncePackage   ->   |
//!   |  <- SignShare            |
//!   |  SignAggregate      ->   |
//! ```
//!
//! Retries rotate the leader and exclude missing signers when timeouts occur.

pub mod core;
mod engine;
mod error;
pub mod storage;

pub use crate::policy::select_roast_leader as select_leader;
pub use core::{
    ParticipantAction, ParticipantConfig, ParticipantEvent, RoastParticipant, RoastResult,
    SessionConfig,
};
pub use engine::{RoastEngine, RoastEngineEvent};
pub use error::{RoastErrorExt, RoastErrorKind};
pub use storage::{
    CoordinatorAction, CoordinatorConfig, CoordinatorEvent, RoastCoordinator, RoastManager,
    RoastMessage,
};
