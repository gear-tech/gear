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

use crate::{
    OutputTask, TxHashBlake2b256, TxPoolOutputTaskSender, TxReferenceBlockHash, TxSignature,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ethexe_db::Database;
use ethexe_signer::ToDigest;
use parity_scale_codec::Encode;
use std::mem;
use tokio::sync::oneshot;

type BoxedStatelessValidator<Tx> = Box<dyn StatelessValidation<Tx>>;
type BoxedStatefulValidator<Tx> = Box<dyn StatefulValidation<Tx>>;
type BoxedAsyncValidator<Tx> = Box<dyn AsyncValidation<Tx>>;

// TODO #4424

/// Transaction validation which doesn;t require any state to be known.
trait StatelessValidation<Tx>: Send + Sync {
    fn validate(&self, tx: &Tx) -> Result<()>;
}

/// Transaction validation which requires db state to be known.
trait StatefulValidation<Tx>: Send + Sync {
    fn validate(&self, tx: &Tx, db: &Database) -> Result<()>;
}

/// General asynchronous transaction validation.
///
/// The trait is provided for all the tx validators, which
/// require async computation for the traansaction validation.
#[async_trait]
trait AsyncValidation<Tx>: Send + Sync {
    async fn async_validate(&self, tx: &Tx) -> Result<()>;
}

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
pub(crate) struct TxValidator<Tx> {
    transaction: Tx,
    db: Database,
    stateless_validators: Vec<BoxedStatelessValidator<Tx>>,
    stateful_validators: Vec<BoxedStatefulValidator<Tx>>,
    async_validators: Vec<BoxedAsyncValidator<Tx>>,
}

/// Trait to finish the validation process and get the transaction back
/// on the `Result<TxValidator<Tx>>`` type.
pub(crate) trait TxValidatorFinishResult<Tx> {
    /// Finish the transaction validation process and get the transaction back.
    fn finish_validator_res(self) -> Result<Tx>;
}

impl<Tx> TxValidatorFinishResult<Tx> for Result<TxValidator<Tx>> {
    fn finish_validator_res(self) -> Result<Tx> {
        self.map(|this| this.finish())
    }
}

impl<Tx> TxValidator<Tx> {
    pub(crate) fn new(transaction: Tx, db: Database) -> Self {
        Self {
            transaction,
            db,
            stateless_validators: Vec::new(),
            stateful_validators: Vec::new(),
            async_validators: Vec::new(),
        }
    }

    /// Runs all sync and async validators for the transaction.
    pub(crate) async fn full_validate(self) -> Result<Self> {
        let this = self.validate()?;
        this.validate_async().await
    }

    /// Runs all stateful and stateless sync validators for the transaction.
    pub(crate) fn validate(mut self) -> Result<Self> {
        let stateless_validators = mem::take(&mut self.stateless_validators);
        for stateless_validator in stateless_validators {
            stateless_validator.validate(&self.transaction)?;
        }

        let stateful_validators = mem::take(&mut self.stateful_validators);
        for stateful_validator in stateful_validators {
            stateful_validator.validate(&self.transaction, &self.db)?;
        }

        Ok(self)
    }

    /// Runs all async validators for the transaction.
    pub(crate) async fn validate_async(mut self) -> Result<Self> {
        let async_validators = mem::take(&mut self.async_validators);
        for async_validator in async_validators {
            async_validator.async_validate(&self.transaction).await?;
        }

        Ok(self)
    }

    /// Finish the validation process and get the transaction back.
    pub(crate) fn finish(self) -> Tx {
        if !(self.stateful_validators.is_empty()
            && self.stateful_validators.is_empty()
            && self.async_validators.is_empty())
        {
            panic!("Validation not finished");
        }

        self.transaction
    }
}

impl<Tx> TxValidator<Tx>
where
    Tx: TxSignature
        + Encode
        + TxReferenceBlockHash
        + TxHashBlake2b256
        + Clone
        + Send
        + Sync
        + 'static,
{
    /// Include all the checks for the validation module.
    pub(crate) fn with_all_checks(
        self,
        tx_pool_output_task_sender: TxPoolOutputTaskSender<Tx>,
    ) -> Self {
        self.with_signature_check()
            .with_mortality_check()
            .with_uniqueness_check()
            .with_executable_tx_check(tx_pool_output_task_sender)
    }
}

impl<Tx: TxSignature + Encode> TxValidator<Tx> {
    /// Add `SignatureValidator` check.
    pub(crate) fn with_signature_check(mut self) -> Self {
        self.stateless_validators.push(Box::new(SignatureValidator));

        self
    }
}

impl<Tx: TxReferenceBlockHash> TxValidator<Tx> {
    /// Add `MortalityValidator` check.
    pub(crate) fn with_mortality_check(mut self) -> Self {
        self.stateful_validators.push(Box::new(MortalityValidator));

        self
    }
}

impl<Tx: TxHashBlake2b256> TxValidator<Tx> {
    /// Add `UniqunessValidator` check.
    pub(crate) fn with_uniqueness_check(mut self) -> Self {
        self.stateful_validators.push(Box::new(UniqunessValidator));

        self
    }
}

impl<Tx: Clone + Send + Sync + 'static> TxValidator<Tx> {
    /// Add `ExecutableTxValidator` check.
    pub(crate) fn with_executable_tx_check(
        mut self,
        tx_pool_output_task_sender: TxPoolOutputTaskSender<Tx>,
    ) -> Self {
        self.async_validators.push(Box::new(ExecutableTxValidator {
            tx_pool_output_task_sender,
        }));

        self
    }
}

/// Validates transaction signature.
pub(crate) struct SignatureValidator;

impl<Tx: TxSignature + Encode> StatelessValidation<Tx> for SignatureValidator {
    fn validate(&self, tx: &Tx) -> Result<()> {
        let tx_digest = tx.encode().to_digest();
        let signature = tx.signature()?;

        signature.verify_with_public_key_recover(tx_digest)
    }
}

/// Validates transaction mortality.
///
/// Basically checks that transaction reference block hash is within the recent blocks window.
pub(crate) struct MortalityValidator;

impl<Tx: TxReferenceBlockHash> StatefulValidation<Tx> for MortalityValidator {
    fn validate(&self, tx: &Tx, db: &Database) -> Result<()> {
        let block_hash = tx.reference_block_hash();

        if db.check_within_recent_blocks(block_hash) {
            Ok(())
        } else {
            Err(anyhow!("Transaction out of recent blocks window"))
        }
    }
}

/// Validates transaction uniqueness.
///
/// Basically checks that transaction is not already in the database.
pub(crate) struct UniqunessValidator;

impl<Tx: TxHashBlake2b256> StatefulValidation<Tx> for UniqunessValidator {
    fn validate(&self, tx: &Tx, db: &Database) -> Result<()> {
        let tx_hash = tx.tx_hash();

        if db.validated_transaction(tx_hash).is_none() {
            Ok(())
        } else {
            Err(anyhow!("Transaction already exists"))
        }
    }
}

/// Validates if transaction is executable.
///
/// Basically sends the transaction to the external transaction pool service
/// to check if it is executable.
pub(crate) struct ExecutableTxValidator<Tx> {
    tx_pool_output_task_sender: TxPoolOutputTaskSender<Tx>,
}

#[async_trait]
impl<Tx: Clone + Send + Sync + 'static> AsyncValidation<Tx> for ExecutableTxValidator<Tx> {
    async fn async_validate(&self, tx: &Tx) -> Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();
        let output_task = OutputTask::CheckIsExecutableTransaction {
            transaction: tx.clone(),
            response_sender,
        };
        self.tx_pool_output_task_sender
            .send(output_task)
            .unwrap_or_else(|e| {
                // If receiving end of the external service is dropped, it's a panic case,
                // because otherwise transaction validation can't be performed correctly.
                //
                // Error should not be returned, as error signalizes that a transaction
                // is invalid, but that's not the case here.
                let err_msg = format!(
                    "Failed to send task to validate if tx is executable: {e}. \
                The receiving end in the tx pool might have been dropped."
                );

                log::error!("{err_msg}");
                panic!("{err_msg}");
            });

        let res = response_receiver.await.unwrap_or_else(|e| {
            // If the response sender on the external service side is dropped, it's a panic case,
            // because otherwise transaction validation can't be performed correctly, as it's
            // unknown if the transaction is executable.
            //
            // Error should not be returned, as error signalizes that a transaction
            // is invalid, but that's not the case here.
            let err_msg =
                format!("Failed to receive from external service if tx is executable: {e}");

            log::error!("{err_msg}");
            panic!("{err_msg}");
        });

        res.then_some(())
            .ok_or(anyhow!("Transaction is not executable"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tests, TxPoolOutputTaskReceiver};
    use ethexe_db::{BlockMetaStorage, Database, MemDb};
    use gprimitives::H256;
    use parity_scale_codec::Encode;
    use tokio::sync::mpsc;


    #[test]
    fn test_signature_validation() {
        let signed_transaction = tests::signed_ethexe_tx(H256::random());
        assert!(SignatureValidator.validate(&signed_transaction).is_ok());
    }

    #[test]
    fn test_valid_mortality() {
        let db = Database::from_one(&MemDb::default(), Default::default());

        // Test valid mortality
        let block_data = tests::random_block();
        db.set_latest_valid_block(block_data.0, block_data.1);

        let (block_hash, header) = tests::random_block();
        db.set_latest_valid_block(block_hash, header);

        let signed_tx = tests::signed_ethexe_tx(block_hash);

        let block_data = tests::random_block();
        db.set_latest_valid_block(block_data.0, block_data.1);

        // Check on plain transaction
        assert!(MortalityValidator
            .validate(&signed_tx.transaction, &db)
            .is_ok());

        // Check on signed transaction
        assert!(MortalityValidator.validate(&signed_tx, &db).is_ok());
    }

    #[test]
    fn test_invalid_mortality_non_existent_block() {
        let db = Database::from_one(&MemDb::default(), Default::default());
        let non_window_block_hash = H256::random();
        let invalid_transaction = tests::signed_ethexe_tx(non_window_block_hash);
        assert!(MortalityValidator
            .validate(&invalid_transaction, &db)
            .is_err());
    }

    #[test]
    fn test_invalid_mortality_rotten_tx() {
        let db = Database::from_one(&MemDb::default(), Default::default());

        let (first_block_hash, first_block_header) = tests::random_block();
        db.set_latest_valid_block(first_block_hash, first_block_header);
        let (second_block_hash, second_block_header) = tests::random_block();
        db.set_latest_valid_block(second_block_hash, second_block_header);

        // Add more 28 blocks
        for _ in 0..28 {
            let block_data = tests::random_block();
            db.set_latest_valid_block(block_data.0, block_data.1);
        }

        let transaction1 = tests::signed_ethexe_tx(first_block_hash);
        assert!(MortalityValidator.validate(&transaction1, &db).is_ok());
        let transaction2 = tests::signed_ethexe_tx(second_block_hash);
        assert!(MortalityValidator.validate(&transaction2, &db).is_ok());

        // Adding a new block to the db, which should remove the first block from window
        let block_data = tests::random_block();
        db.set_latest_valid_block(block_data.0, block_data.1);
        assert!(MortalityValidator.validate(&transaction1, &db).is_err());
        assert!(MortalityValidator.validate(&transaction2, &db).is_ok());
    }

    #[test]
    fn test_uniqueness_validation() {
        let db = Database::from_one(&MemDb::default(), Default::default());
        let transaction = tests::signed_ethexe_tx(H256::random());

        assert!(UniqunessValidator.validate(&transaction, &db).is_ok());

        db.set_validated_transaction(transaction.tx_hash(), transaction.encode());

        assert!(UniqunessValidator.validate(&transaction, &db).is_err());
    }

    #[tokio::test]
    async fn test_executable_tx_validation() {
        let run_executable_tx_validation = |response_value| async move {
            let (sender, receiver) = mpsc::unbounded_channel();
            let output_task_sender = TxPoolOutputTaskSender { sender };
            let mut output_task_receiver = TxPoolOutputTaskReceiver { receiver };

            let validator = ExecutableTxValidator {
                tx_pool_output_task_sender: output_task_sender,
            };

            // Spawn a thread for tx pool service
            tokio::spawn(async move {
                let task = output_task_receiver
                    .recv()
                    .await
                    .expect("failed receiving task");
                match task {
                    OutputTask::CheckIsExecutableTransaction {
                        response_sender, ..
                    } => {
                        response_sender
                            .send(response_value)
                            .expect("failed sending response");
                    }
                    _ => unreachable!("unexpected task"),
                }
            });

            let transaction = tests::signed_ethexe_tx(H256::random());
            validator.async_validate(&transaction).await
        };

        // Test valid transaction
        assert!(run_executable_tx_validation(true).await.is_ok());

        // Test invalid transaction
        assert!(run_executable_tx_validation(false).await.is_err());
    }

    // TODO [sab]:
    // 2) general tx pool service test
    // 3) general serivce test (ethexe/cli)
}
