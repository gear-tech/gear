// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use sp_blockchain::Error;

/// Subscription management error.
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionManagementError {
    /// The subscription has exceeded the internal limits
    /// regarding the number of pinned blocks in memory or
    /// the number of ongoing operations.
    #[error("Exceeded pinning or operation limits")]
    ExceededLimits,
    /// Error originated from the blockchain (client or backend).
    #[error("Blockchain error {0}")]
    Blockchain(Error),
    /// The database does not contain a block hash.
    #[error("Block hash is absent")]
    BlockHashAbsent,
    /// The database does not contain a block header.
    #[error("Block header is absent")]
    BlockHeaderAbsent,
    /// The specified subscription ID is not present.
    #[error("Subscription is absent")]
    SubscriptionAbsent,
    /// The unpin method was called with duplicate hashes.
    #[error("Duplicate hashes")]
    DuplicateHashes,
    /// The distance between the leaves and the current finalized block is too large.
    #[error("Distance too large")]
    BlockDistanceTooLarge,
    /// Custom error.
    #[error("Subscription error {0}")]
    Custom(String),
}

// Blockchain error does not implement `PartialEq` needed for testing.
impl PartialEq for SubscriptionManagementError {
    fn eq(&self, other: &SubscriptionManagementError) -> bool {
        match (self, other) {
			(Self::ExceededLimits, Self::ExceededLimits) |
			// Not needed for testing.
			(Self::Blockchain(_), Self::Blockchain(_)) |
			(Self::BlockHashAbsent, Self::BlockHashAbsent) |
			(Self::BlockHeaderAbsent, Self::BlockHeaderAbsent) |
			(Self::SubscriptionAbsent, Self::SubscriptionAbsent) |
			(Self::DuplicateHashes, Self::DuplicateHashes) => true,
			(Self::BlockDistanceTooLarge, Self::BlockDistanceTooLarge) => true,
			(Self::Custom(lhs), Self::Custom(rhs)) => lhs == rhs,
			_ => false,
		}
    }
}

impl From<Error> for SubscriptionManagementError {
    fn from(err: Error) -> Self {
        SubscriptionManagementError::Blockchain(err)
    }
}
