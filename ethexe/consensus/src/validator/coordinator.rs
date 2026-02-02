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

use super::{StateHandler, ValidatorContext, ValidatorState};
use crate::{
    BatchCommitmentValidationReply, CommitmentSubmitted, ConsensusEvent,
    utils::MultisignedBatchCommitment, validator::initial::Initial,
};
use anyhow::{Result, anyhow, ensure};
use derive_more::Display;
use ethexe_common::{
    Address, ToDigest, ValidatorsVec, consensus::BatchCommitmentValidationRequest,
    gear::BatchCommitment, network::ValidatorMessage,
};
use futures::FutureExt;
use gsigner::secp256k1::Secp256k1SignerExt;
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
                    .ok_or_else(|| anyhow!("Received validation reply is not known validator"))
            })
        {
            self.warning(format!("validation reply rejected: {err}"));
        }

        if self.multisigned_batch.signatures().len() as u64 >= self.ctx.core.signatures_threshold {
            Self::submission(self.ctx, self.multisigned_batch)
        } else {
            Ok(self.into())
        }
    }
}

impl Coordinator {
    pub fn create(
        mut ctx: ValidatorContext,
        validators: ValidatorsVec,
        batch: BatchCommitment,
    ) -> Result<ValidatorState> {
        ensure!(
            validators.len() as u64 >= ctx.core.signatures_threshold,
            "Number of validators is less than threshold"
        );

        ensure!(
            ctx.core.signatures_threshold > 0,
            "Threshold should be greater than 0"
        );

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &ctx.core.signer,
            ctx.core.router_address,
            ctx.core.pub_key,
        )?;

        if multisigned_batch.signatures().len() as u64 >= ctx.core.signatures_threshold {
            return Self::submission(ctx, multisigned_batch);
        }

        let era_index = ctx
            .core
            .timelines
            .era_from_ts(multisigned_batch.batch().timestamp);
        let payload = BatchCommitmentValidationRequest::new(multisigned_batch.batch());
        let message = ValidatorMessage { era_index, payload };

        let validation_request = ctx
            .core
            .signer
            .signed_data(ctx.core.pub_key, message, None)?;

        ctx.output(ConsensusEvent::PublishMessage(validation_request.into()));

        Ok(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }
        .into())
    }

    pub fn submission(
        ctx: ValidatorContext,
        multisigned_batch: MultisignedBatchCommitment,
    ) -> Result<ValidatorState> {
        let (batch, signatures) = multisigned_batch.into_parts();
        let cloned_committer = ctx.core.committer.clone_boxed();
        ctx.tasks.push(
            async move {
                let block_hash = batch.block_hash;
                let batch_digest = batch.to_digest();
                let event = match cloned_committer.commit(batch, signatures).await {
                    Ok(tx) => CommitmentSubmitted {
                        block_hash,
                        batch_digest,
                        tx,
                    }.into(),
                    Err(err) => ConsensusEvent::Warning(format!(
                        "Failed to submit commitment for block {block_hash}, digest {batch_digest}: {err}"
                    ))
                };
                Ok(event)
            }
            .boxed(),
        );
        Initial::create(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::{ToDigest, ValidatorsVec};
    use gprimitives::H256;
    use nonempty::NonEmpty;

    #[test]
    fn coordinator_create_success() {
        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 2;
        let validators: ValidatorsVec = keys
            .iter()
            .take(3)
            .map(|k| k.to_address())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let batch = BatchCommitment::default();

        let coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert!(coordinator.is_coordinator());
        coordinator.context().output[0]
            .clone()
            .unwrap_publish_message()
            .unwrap_request_batch_validation();
    }

    #[test]
    fn coordinator_create_insufficient_validators() {
        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 3;
        let validators =
            NonEmpty::from_vec(keys.iter().take(2).map(|k| k.to_address()).collect()).unwrap();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators.into(), batch).is_err(),
            "Expected an error, but got Ok"
        );
    }

    #[test]
    fn coordinator_create_zero_threshold() {
        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 0;
        let validators =
            NonEmpty::from_vec(keys.iter().take(1).map(|k| k.to_address()).collect()).unwrap();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators.into(), batch).is_err(),
            "Expected an error due to zero threshold, but got Ok"
        );
    }

    #[test]
    fn process_validation_reply() {
        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 3;
        let validators =
            NonEmpty::from_vec(keys.iter().take(3).map(|k| k.to_address()).collect()).unwrap();

        let batch = BatchCommitment::default();
        let digest = batch.to_digest();

        let reply1 = ctx
            .core
            .signer
            .validation_reply(keys[0], ctx.core.router_address, digest);

        let reply2_invalid =
            ctx.core
                .signer
                .validation_reply(keys[4], ctx.core.router_address, digest);

        let reply3_invalid = ctx.core.signer.validation_reply(
            keys[1],
            ctx.core.router_address,
            H256::random().0.into(),
        );

        let reply4 = ctx
            .core
            .signer
            .validation_reply(keys[2], ctx.core.router_address, digest);

        let mut coordinator = Coordinator::create(ctx, validators.into(), batch).unwrap();
        assert!(coordinator.is_coordinator());
        coordinator.context().output[0]
            .clone()
            .unwrap_publish_message()
            .unwrap_request_batch_validation();

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
        assert!(coordinator.is_initial());
        assert_eq!(coordinator.context().output.len(), 3);
        assert!(coordinator.context().tasks.len() == 1);
    }
}
