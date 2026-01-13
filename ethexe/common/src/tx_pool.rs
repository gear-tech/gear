// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{hash::HashOf, injected::InjectedTransaction};
use gprimitives::ActorId;

/// The status of [`InjectedTransaction`] for specific announce and chain head.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::From, derive_more::Display)]
pub enum TransactionStatus {
    /// Transaction is valid and can be include into announce.
    Valid,
    /// Transaction is in pending status ([`PendingStatus`]).
    #[from]
    Pending(PendingStatus),
    /// Transaction is not valid.
    /// The [`RemovalNotification`] will be returned to the transaction's sender.
    #[from]
    Invalid(InvalidReason),
}

/// The pending status means that the transaction is not valid now, but
/// it may become valid in the future (e.g., after a reorg).
///
/// In this status, the transaction should be kept in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum PendingStatus {
    // If transaction was already included in some announce we keep it in pool, because of chain reorgs.
    #[display("Transaction with the same hash was already included")]
    AlreadyIncluded,
    // If transaction's reference block is not on current branch we keep it in pool, because of chain reorgs.
    #[display("Transaction's reference block is not on current branch")]
    NotOnCurrentBranch,
    /// In case when transaction is sent to uninitialized actor, we keep it in pool,
    /// because in next blocks actor can be initialized.
    #[display("Transaction's destination actor({destination}) is uninitialized")]
    UninitializedDestination { destination: ActorId },
}

/// The reason why the transaction is not valid and cannot be included into announce.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum InvalidReason {
    #[display("Transaction was not included within validity window and becomes outdated")]
    Outdated,
    #[display("Transaction's destination actor({destination}) not found")]
    UnknownDestination { destination: ActorId },
}

/// Notification about removed transaction from the pool.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalNotification {
    // Removed transaction hash
    pub tx_hash: HashOf<InjectedTransaction>,
    // The reason why it is removed
    pub reason: InvalidReason,
}
