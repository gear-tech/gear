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

//! Transactions validation.

use crate::SignedOffchainTransaction;
use anyhow::{anyhow, bail, Context, Result};
use ethexe_db::Database;

// TODO #4424

/// Main transaction pool tx validator.
///
/// Basically consumes a transaction and runs all the defined checks on it.
/// The checks are defined through the `with_*_check` methods.
pub(crate) struct TxValidator {
    transaction: SignedOffchainTransaction,
    db: Database,
    mortality_check: bool,
    uniqueness_check: bool,
}

impl TxValidator {
    pub(crate) fn new(transaction: SignedOffchainTransaction, db: Database) -> Self {
        Self {
            transaction,
            db,
            mortality_check: false,
            uniqueness_check: false,
        }
    }

    pub(crate) fn with_all_checks(self) -> Self {
        self.with_mortality_check().with_uniqueness_check()
    }

    pub(crate) fn with_mortality_check(mut self) -> Self {
        self.mortality_check = true;
        self
    }

    pub(crate) fn with_uniqueness_check(mut self) -> Self {
        self.uniqueness_check = true;
        self
    }
}

impl TxValidator {
    /// Runs all stateful and stateless sync validators for the transaction.
    pub(crate) fn validate(self) -> Result<SignedOffchainTransaction> {
        if self.mortality_check && !self.check_mortality()? {
            bail!("Transaction reference block hash is out of recent blocks window");
        }

        if self.uniqueness_check {
            self.check_uniqueness()?;
        }

        Ok(self.transaction)
    }

    /// Validates transaction mortality.
    ///
    /// Basically checks that transaction reference block hash is within the recent blocks window.
    fn check_mortality(&self) -> Result<bool> {
        let block_hash = self.transaction.reference_block();

        self.db
            .check_within_recent_blocks(block_hash)
            .context("Failed to perform mortality check")
    }

    /// Validates transaction uniqueness.
    ///
    /// Basically checks that transaction is not already in the database.
    fn check_uniqueness(&self) -> Result<()> {
        let tx_hash = self.transaction.tx_hash();

        // TODO #4505
        if self.db.get_offchain_transaction(tx_hash).is_none() {
            Ok(())
        } else {
            Err(anyhow!("Transaction already exists"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{self, BlocksManager};
    use ethexe_db::Database;
    use gprimitives::H256;

    macro_rules! assert_ok {
        ( $x:expr ) => {
            assert!($x.is_ok());
        };
    }

    macro_rules! assert_err {
        ( $x:expr ) => {
            assert!($x.is_err());
        };
    }

    #[test]
    fn test_valid_mortality() {
        let db = Database::memory();
        let bm = BlocksManager::new(db.clone());

        // Test valid mortality
        bm.add_block();
        let (block_hash, _) = bm.add_block();

        let signed_tx = tests::generate_signed_ethexe_tx(block_hash);

        bm.add_block();

        let tx_validator = TxValidator::new(signed_tx, db).with_mortality_check();
        assert_ok!(tx_validator.validate());
    }

    #[test]
    fn test_invalid_mortality_non_existent_block() {
        let db = Database::memory();
        let non_window_block_hash = H256::random();
        let invalid_transaction = tests::generate_signed_ethexe_tx(non_window_block_hash);

        let tx_validator = TxValidator::new(invalid_transaction, db).with_mortality_check();

        assert_err!(tx_validator.validate());
    }

    #[test]
    fn test_invalid_mortality_rotten_tx() {
        let db = Database::memory();
        let bm = BlocksManager::new(db.clone());

        let first_block_hash = bm.add_block().0;
        let second_block_hash = bm.add_block().0;

        // Add more 30 blocks
        (0..30).for_each(|_| {
            bm.add_block();
        });

        let transaction1 = TxValidator::new(
            tests::generate_signed_ethexe_tx(first_block_hash),
            db.clone(),
        )
        .with_mortality_check()
        .validate()
        .expect("internal error: transaction1 validation failed");

        let transaction2 = TxValidator::new(
            tests::generate_signed_ethexe_tx(second_block_hash),
            db.clone(),
        )
        .with_mortality_check()
        .validate()
        .expect("internal error: transaction2 validation failed");

        // Adding a new block to the db, which should remove the first block from window
        bm.add_block();

        // `db` is `Arc`, so no need to instantiate a new validator.
        assert_err!(TxValidator::new(transaction1, db.clone())
            .with_mortality_check()
            .validate());
        assert_ok!(TxValidator::new(transaction2, db.clone())
            .with_mortality_check()
            .validate());
    }

    #[test]
    fn test_uniqueness_validation() {
        let db = Database::memory();
        let transaction = tests::generate_signed_ethexe_tx(H256::random());

        let transaction = TxValidator::new(transaction, db.clone())
            .with_uniqueness_check()
            .validate()
            .expect("internal error: uniqueness validation failed");

        db.set_offchain_transaction(transaction.clone());

        assert_err!(TxValidator::new(transaction, db.clone())
            .with_uniqueness_check()
            .validate());
    }
}
