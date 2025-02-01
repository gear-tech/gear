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

//! Transactions validation.

use crate::SignedTransaction;
use anyhow::{anyhow, Result};
use ethexe_db::Database;
use ethexe_processor::Processor;
use ethexe_signer::ToDigest;
use parity_scale_codec::Encode;

// TODO #4424

/// Main transaction pool tx validator.
///
/// Basically consumes a transaction and runs all the defined checks on it.
/// The checks are defined through the `with_*_check` methods.
///
/// The transaction is given back by the validator only in case of
/// all checks passing. A corresponding `finish` method must be called.
///
/// The validator is considered to be called by the transaciton pool service,
/// so sub-validators can send specific validation tasks outside to the service.
pub(crate) struct TxValidator {
    transaction: SignedTransaction,
    db: Database,
    signature_check: bool,
    mortality_check: bool,
    uniqueness_check: bool,
    executable_tx_check: bool,
}

impl TxValidator {
    pub(crate) fn new(transaction: SignedTransaction, db: Database) -> Self {
        Self {
            transaction,
            db,
            signature_check: false,
            mortality_check: false,
            uniqueness_check: false,
            executable_tx_check: false,
        }
    }

    pub(crate) fn with_all_checks(self) -> Self {
        self.with_signature_check()
            .with_mortality_check()
            .with_uniqueness_check()
            .with_executable_tx_check()
    }

    pub(crate) fn with_signature_check(mut self) -> Self {
        self.signature_check = true;
        self
    }

    pub(crate) fn with_mortality_check(mut self) -> Self {
        self.mortality_check = true;
        self
    }

    pub(crate) fn with_uniqueness_check(mut self) -> Self {
        self.uniqueness_check = true;
        self
    }

    pub(crate) fn with_executable_tx_check(mut self) -> Self {
        self.executable_tx_check = true;
        self
    }
}

impl TxValidator {
    /// Runs all stateful and stateless sync validators for the transaction.
    pub(crate) fn validate(self) -> Result<SignedTransaction> {
        if self.signature_check {
            self.check_signature()?;
        }

        if self.mortality_check {
            self.check_mortality()?;
        }

        if self.uniqueness_check {
            self.check_uniqueness()?;
        }

        if self.executable_tx_check {
            self.check_is_executable_tx()?;
        }

        Ok(self.transaction)
    }

    /// Validates transaction signature.
    fn check_signature(&self) -> Result<()> {
        let tx_digest = self.transaction.encode().to_digest();
        let signature = self.transaction.signature()?;

        signature.verify_with_public_key_recover(tx_digest)
    }

    /// Validates transaction mortality.
    ///
    /// Basically checks that transaction reference block hash is within the recent blocks window.
    fn check_mortality(&self) -> Result<()> {
        let block_hash = self.transaction.reference_block_hash();

        if self.db.check_within_recent_blocks(block_hash) {
            Ok(())
        } else {
            Err(anyhow!("Transaction out of recent blocks window"))
        }
    }

    /// Validates transaction uniqueness.
    ///
    /// Basically checks that transaction is not already in the database.
    fn check_uniqueness(&self) -> Result<()> {
        let tx_hash = self.transaction.tx_hash();

        if self.db.validated_transaction(tx_hash).is_none() {
            Ok(())
        } else {
            Err(anyhow!("Transaction already exists"))
        }
    }

    /// Validates if transaction is executable.
    fn check_is_executable_tx(&self) -> Result<()> {
        // TODO breathx
        let processor = Processor::new(self.db.clone())?;
        let _overlaid_processor = processor.overlaid();

        let _source = self.transaction.send_message_source();

        log::warn!("Unimplemented transaction is executable check");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use ethexe_db::{BlockMetaStorage, Database, MemDb};
    use gprimitives::H256;
    use parity_scale_codec::Encode;

    #[test]
    fn test_signature_validation() {
        let signed_transaction = tests::generate_signed_ethexe_tx(H256::random());
        let db = Database::from_one(&MemDb::default(), Default::default());
        let validator = TxValidator::new(signed_transaction, db).with_signature_check();
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn test_valid_mortality() {
        let db = Database::from_one(&MemDb::default(), Default::default());

        // Test valid mortality
        let block_data = tests::new_block(None);
        db.set_block_header(block_data.0, block_data.1.clone());
        db.set_latest_valid_block(block_data.0, block_data.1);

        let (block_hash, header) = tests::new_block(Some(block_data.0));
        db.set_block_header(block_hash, header.clone());
        db.set_latest_valid_block(block_hash, header);

        let signed_tx = tests::generate_signed_ethexe_tx(block_hash);

        let block_data = tests::new_block(Some(block_hash));
        db.set_block_header(block_data.0, block_data.1.clone());
        db.set_latest_valid_block(block_data.0, block_data.1);

        let tx_validator = TxValidator::new(signed_tx, db).with_mortality_check();

        assert!(tx_validator.validate().is_ok());
    }

    #[test]
    fn test_invalid_mortality_non_existent_block() {
        let db = Database::from_one(&MemDb::default(), Default::default());
        let non_window_block_hash = H256::random();
        let invalid_transaction = tests::generate_signed_ethexe_tx(non_window_block_hash);

        let tx_validator = TxValidator::new(invalid_transaction, db).with_mortality_check();

        assert!(tx_validator.validate().is_err());
    }

    #[test]
    fn test_invalid_mortality_rotten_tx() {
        let db = Database::from_one(&MemDb::default(), Default::default());

        let (first_block_hash, first_block_header) = tests::new_block(None);
        db.set_block_header(first_block_hash, first_block_header.clone());
        db.set_latest_valid_block(first_block_hash, first_block_header);
        let (second_block_hash, second_block_header) = tests::new_block(Some(first_block_hash));
        db.set_block_header(second_block_hash, second_block_header.clone());
        db.set_latest_valid_block(second_block_hash, second_block_header);

        // Add more 28 blocks
        let mut block_hash = second_block_hash;
        for _ in 0..28 {
            let block_data = tests::new_block(Some(block_hash));
            db.set_block_header(block_data.0, block_data.1.clone());
            db.set_latest_valid_block(block_data.0, block_data.1);
            block_hash = block_data.0;
        }

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
        let block_data = tests::new_block(Some(block_hash));
        db.set_block_header(block_data.0, block_data.1.clone());
        db.set_latest_valid_block(block_data.0, block_data.1);

        // `db` is `Arc`, so no need to instatiate a new validator.
        assert!(TxValidator::new(transaction1, db.clone())
            .with_mortality_check()
            .validate()
            .is_err());
        assert!(TxValidator::new(transaction2, db.clone())
            .with_mortality_check()
            .validate()
            .is_ok());
    }

    #[test]
    fn test_uniqueness_validation() {
        let db = Database::from_one(&MemDb::default(), Default::default());
        let transaction = tests::generate_signed_ethexe_tx(H256::random());

        let transaction = TxValidator::new(transaction, db.clone())
            .with_uniqueness_check()
            .validate()
            .expect("internal error: uniqueness validation failed");

        db.set_validated_transaction(transaction.tx_hash(), transaction.encode());

        assert!(TxValidator::new(transaction, db.clone())
            .with_uniqueness_check()
            .validate()
            .is_err());
    }

    #[test]
    fn test_executable_tx_validation() {
        let transaction = tests::generate_signed_ethexe_tx(H256::random());
        let db = Database::from_one(&MemDb::default(), Default::default());
        let tx_validator = TxValidator::new(transaction, db).with_executable_tx_check();

        assert!(tx_validator.validate().is_ok());
    }
}
