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

// TODO [sab]: may return their own error types

type BoxedStatelessValidator<Tx> = Box<dyn StatelessValidation<Tx>>;
type BoxedStatefulValidator<Tx> = Box<dyn StatefulValidation<Tx>>;
type BoxedAsyncValidator<Tx> = Box<dyn AsyncValidation<Tx>>;

pub struct TxValidator<Tx> {
    transaction: Tx,
    db: Database,
    stateless_validators: Vec<BoxedStatelessValidator<Tx>>,
    stateful_validators: Vec<BoxedStatefulValidator<Tx>>,
    async_validators: Vec<BoxedAsyncValidator<Tx>>,
}

pub trait TxValidatorFinishResult<Tx> {
    fn finish_validator_res(self) -> Result<Tx>;
}

impl<Tx> TxValidatorFinishResult<Tx> for Result<TxValidator<Tx>> {
    fn finish_validator_res(self) -> Result<Tx> {
        self.map(|this| this.finish())
    }
}

impl<Tx> TxValidator<Tx> {
    pub fn new(transaction: Tx, db: Database) -> Self {
        Self {
            transaction,
            db,
            stateless_validators: Vec::new(),
            stateful_validators: Vec::new(),
            async_validators: Vec::new(),
        }
    }

    pub async fn full_validate(self) -> Result<Self> {
        let this = self.validate()?;
        this.validate_async().await
    }

    pub fn validate(mut self) -> Result<Self> {
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

    pub async fn validate_async(mut self) -> Result<Self> {
        let async_validators = mem::take(&mut self.async_validators);
        for async_validator in async_validators {
            async_validator.async_validate(&self.transaction).await?;
        }

        Ok(self)
    }

    pub fn finish(self) -> Tx {
        if !(self.stateful_validators.is_empty()
            && self.stateful_validators.is_empty()
            && self.async_validators.is_empty())
        {
            panic!("Validation not finished");
        }

        self.transaction
    }
}

impl<Tx: TxSignature + Encode> TxValidator<Tx> {
    pub fn with_signature_check(mut self) -> Self {
        self.stateless_validators.push(Box::new(SignatureValidator));

        self
    }
}

impl<Tx: TxReferenceBlockHash> TxValidator<Tx> {
    pub fn with_mortality_check(mut self) -> Self {
        self.stateful_validators.push(Box::new(MortalityValidator));

        self
    }
}

impl<Tx: TxHashBlake2b256> TxValidator<Tx> {
    pub fn with_uniqueness_check(mut self) -> Self {
        self.stateful_validators.push(Box::new(UniqunessValidator));

        self
    }
}

impl<Tx: Clone + Send + Sync + 'static> TxValidator<Tx> {
    pub fn with_executable_tx_check(
        mut self,
        tx_pool_output_task_sender: TxPoolOutputTaskSender<Tx>,
    ) -> Self {
        self.async_validators.push(Box::new(ExecutableTxValidator {
            tx_pool_output_task_sender,
        }));

        self
    }
}

trait StatelessValidation<Tx>: Send + Sync {
    fn validate(&self, tx: &Tx) -> Result<()>;
}

trait StatefulValidation<Tx>: Send + Sync {
    // TODO [sab]: change ethexe_db to some generic
    fn validate(&self, tx: &Tx, db: &Database) -> Result<()>;
}

pub struct SignatureValidator;

impl<Tx: TxSignature + Encode> StatelessValidation<Tx> for SignatureValidator {
    fn validate(&self, tx: &Tx) -> Result<()> {
        let tx_digest = tx.encode().to_digest();
        let signature = tx.signature()?;

        signature.verify_with_public_key_recover(tx_digest)
    }
}

pub struct MortalityValidator;

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

pub struct UniqunessValidator;

impl<Tx: TxHashBlake2b256> StatefulValidation<Tx> for UniqunessValidator {
    fn validate(&self, tx: &Tx, db: &Database) -> Result<()> {
        let tx_hash = tx.tx_hash();

        if db.check_is_unique(tx_hash) {
            Ok(())
        } else {
            Err(anyhow!("Transaction already exists"))
        }
    }
}

#[async_trait]
trait AsyncValidation<Tx>: Send + Sync {
    async fn async_validate(&self, tx: &Tx) -> Result<()>;
}

pub struct ExecutableTxValidator<Tx> {
    tx_pool_output_task_sender: TxPoolOutputTaskSender<Tx>,
}

#[async_trait]
impl<Tx: Clone + Send + Sync + 'static> AsyncValidation<Tx> for ExecutableTxValidator<Tx> {
    async fn async_validate(&self, tx: &Tx) -> Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();
        let outout_task = OutputTask::CheckIsExecutable {
            transaction: tx.clone(),
            response_sender,
        };
        self.tx_pool_output_task_sender
            .send(outout_task)
            .inspect_err(|e| {
                log::error!(
                    "Failed to send task to validate if tx is executable: {e}. \
                The receiving end in the tx pool might have been dropped."
                );
            })?;

        let res = response_receiver.await.inspect_err(|e| {
            log::error!("Failed to receive from external service if tx is executable: {e}");
        })?;

        res.then_some(())
            .ok_or(anyhow!("Transaction is not executable"))
    }
}
