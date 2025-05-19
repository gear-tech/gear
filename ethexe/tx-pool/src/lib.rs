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

//! Ethexe transaction pool.

mod validation;

#[cfg(test)]
mod tests;

use anyhow::{Context as _, Result};
pub use ethexe_common::tx_pool::{
    OffchainTransaction, RawOffchainTransaction, SignedOffchainTransaction,
};
use ethexe_db::Database;
use gprimitives::{ActorId, H160};
use validation::TxValidator;

/// Transaction pool service.
///
/// Serves as an interface for the transaction pool core.
pub struct TxPoolService {
    db: Database,
}

impl TxPoolService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Basically validates the transaction and includes the transaction
    /// to the ready queue, so it's returned by the service stream.
    pub fn validate(
        &self,
        transaction: SignedOffchainTransaction,
    ) -> Result<SignedOffchainTransaction> {
        TxValidator::new(transaction, self.db.clone())
            .with_all_checks()
            .validate()
            .context("Tx validation failed")
    }
}

/// Gets source of the `SendMessage` transaction recovering it from the signature.
pub fn tx_send_message_source(tx: &SignedOffchainTransaction) -> ActorId {
    H160(tx.public_key().to_address().0).into()
}
