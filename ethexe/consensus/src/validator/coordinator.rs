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

use super::{
    submitter::{Submitter, SubmitterError},
    StateHandler, ValidatorContext, ValidatorState,
};
use crate::{
    utils::{BatchCommitmentValidationRequest, MultisignedBatchCommitment},
    BatchCommitmentValidationReply, ConsensusEvent,
};
// use anyhow::{anyhow, ensure, Result};
use derive_more::{Debug, Display};
use ethexe_common::{gear::BatchCommitment, Address};
use ethexe_signer::SignerError;
use std::collections::BTreeSet;

/// [`Coordinator`] sends batch commitment validation request to other validators
/// and waits for validation replies.
/// Switches to [`Submitter`], after receiving enough validators replies from other validators.
#[derive(Debug, Display)]
#[display("COORDINATOR")]
pub struct Coordinator {
    ctx: ValidatorContext,
    validators: BTreeSet<Address>,
    multisigned_batch: MultisignedBatchCommitment,
}

#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error("received validation reply is not known validator")]
    UnknownValidator(Address),
    #[error(
        "number of validators is less than threshold: expected: {threshold}, got: {validators}"
    )]
    InsufficientValidators { threshold: u64, validators: usize },
    #[error("signatures threshold should be greater then 0")]
    ZeroThreshold,

    #[error("submitter error: {0}")]
    Submitter(#[from] SubmitterError),

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),
}

type Result<T> = std::result::Result<T, CoordinatorError>;

impl StateHandler for Coordinator {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_reply(
        mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        if let Err(err) = self
            .multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or(CoordinatorError::UnknownValidator(addr))
            })
        {
            self.warning(format!("validation reply rejected: {err}"));
        }

        if self.multisigned_batch.signatures().len() as u64 >= self.ctx.signatures_threshold {
            Ok(Submitter::create(self.ctx, self.multisigned_batch)?)
        } else {
            Ok(self.into())
        }
    }
}

impl Coordinator {
    pub fn create(
        mut ctx: ValidatorContext,
        validators: Vec<Address>,
        batch: BatchCommitment,
    ) -> Result<ValidatorState> {
        if validators.len() as u64 >= ctx.signatures_threshold {
            return Err(CoordinatorError::InsufficientValidators {
                threshold: ctx.signatures_threshold,
                validators: validators.len(),
            });
        }

        if ctx.signatures_threshold == 0 {
            return Err(CoordinatorError::ZeroThreshold);
        }

        let multisigned_batch =
            MultisignedBatchCommitment::new(batch, &ctx.signer, ctx.router_address, ctx.pub_key)?;

        if multisigned_batch.signatures().len() as u64 >= ctx.signatures_threshold {
            return Ok(Submitter::create(ctx, multisigned_batch)?);
        }

        let validation_request = ctx.signer.signed_data(
            ctx.pub_key,
            BatchCommitmentValidationRequest::new(multisigned_batch.batch()),
        )?;

        ctx.output(ConsensusEvent::PublishValidationRequest(validation_request));

        Ok(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::{Digest, ToDigest};
    use gprimitives::H256;

    #[test]
    fn coordinator_create_success() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.signatures_threshold = 2;
        let validators: Vec<_> = keys.iter().take(3).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        let coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            coordinator.context().output[0],
            ConsensusEvent::PublishValidationRequest(_)
        ));
    }

    #[test]
    fn coordinator_create_insufficient_validators() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.signatures_threshold = 3;
        let validators = keys.iter().take(2).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators, batch).is_err(),
            "Expected an error, but got Ok"
        );
    }

    #[test]
    fn coordinator_create_zero_threshold() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.signatures_threshold = 0;
        let validators: Vec<_> = keys.iter().take(1).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators, batch).is_err(),
            "Expected an error due to zero threshold, but got Ok"
        );
    }

    #[test]
    fn process_validation_reply() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.signatures_threshold = 3;
        let validators: Vec<_> = keys.iter().take(3).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();
        let digest = batch.to_digest();

        let reply1 = mock_validation_reply(&ctx.signer, keys[0], ctx.router_address, digest);
        let reply2_invalid =
            mock_validation_reply(&ctx.signer, keys[4], ctx.router_address, digest);
        let reply3_invalid = mock_validation_reply(
            &ctx.signer,
            keys[1],
            ctx.router_address,
            Digest(H256::random().0),
        );
        let reply4 = mock_validation_reply(&ctx.signer, keys[2], ctx.router_address, digest);

        let mut coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            coordinator.context().output[0],
            ConsensusEvent::PublishValidationRequest(_)
        ));

        coordinator = coordinator.process_validation_reply(reply1).unwrap();
        assert!(coordinator.is_coordinator());

        coordinator = coordinator
            .process_validation_reply(reply2_invalid)
            .unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            coordinator.context().output[1],
            ConsensusEvent::Warning(_)
        ));

        coordinator = coordinator
            .process_validation_reply(reply3_invalid)
            .unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            coordinator.context().output[2],
            ConsensusEvent::Warning(_)
        ));

        coordinator = coordinator.process_validation_reply(reply4).unwrap();
        assert!(coordinator.is_submitter());
        assert_eq!(coordinator.context().output.len(), 3);
    }
}
