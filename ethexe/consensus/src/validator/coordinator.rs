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

use super::{StateHandler, ValidatorContext, ValidatorState, sign_roast_message};
use crate::{
    BatchCommitmentValidationReply, CommitmentSubmitted, ConsensusEvent,
    engine::prelude::RoastEngineEvent, validator::initial::Initial,
};
use anyhow::{Result, anyhow, ensure};
use derive_more::Display;
use ethexe_common::{Address, Digest, ToDigest, ValidatorsVec, gear::BatchCommitment};
use futures::FutureExt;
use gprimitives::{ActorId, H256};
#[cfg(test)]
use gsigner::ContractSignature;
use gsigner::hash::keccak256_iter;
use std::collections::BTreeSet;

/// [`Coordinator`] initiates ROAST threshold signing for batch commitment.
/// Waits for threshold signature to be completed, then switches to submission.
#[derive(Debug, Display)]
#[display("COORDINATOR")]
pub struct Coordinator {
    ctx: ValidatorContext,
    validators: BTreeSet<Address>,
    pub(crate) batch: BatchCommitment,
    pub(crate) batch_digest: Digest,
    pub(crate) signing_hash: H256,
    pub(crate) era_index: u64,
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
        self,
        _reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        // Validation replies are no longer used with ROAST
        // ROAST threshold signing handles coordination
        tracing::trace!("Ignoring validation reply - using ROAST threshold signing");
        Ok(self.into())
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

        let era_index = ctx.core.timelines.era_from_ts(batch.timestamp);
        let batch_digest = batch.to_digest();

        tracing::info!(
            era = era_index,
            block_hash = %batch.block_hash,
            "🔐 Starting ROAST threshold signing for batch commitment"
        );

        // Start ROAST signing session
        // Convert Digest to H256 for ROAST
        let contract_digest = keccak256_iter([
            &[0x19, 0x00],
            ctx.core.router_address.0.as_ref(),
            batch_digest.0.as_ref(),
        ]);
        let msg_hash = H256(contract_digest);

        let tweak_target = ActorId::zero();
        let threshold = ctx.core.signatures_threshold as u16;
        let participants: Vec<Address> = validators.clone().into();

        let messages = ctx
            .roast_engine
            .handle_event(RoastEngineEvent::StartSigning {
                msg_hash,
                era: era_index,
                tweak_target,
                threshold,
                participants,
            })?;

        // Broadcast ROAST session request
        for msg in messages {
            let signed = sign_roast_message(&ctx.core.signer, ctx.core.pub_key, msg)?;
            ctx.output(ConsensusEvent::BroadcastValidatorMessage(signed));
        }

        Ok(Self {
            ctx,
            validators: validators.into_iter().collect(),
            batch,
            batch_digest,
            signing_hash: msg_hash,
            era_index,
        }
        .into())
    }

    /// Called when ROAST threshold signature is complete
    pub fn on_signature_complete(self) -> Result<ValidatorState> {
        // Get the threshold signature from RoastEngine
        let signature = self
            .ctx
            .roast_engine
            .get_signature(self.signing_hash, self.era_index)
            .ok_or_else(|| anyhow!("Signature not found after completion"))?;

        Self::submission_frost(self.ctx, self.batch, signature)
    }

    pub fn submission_frost(
        ctx: ValidatorContext,
        batch: BatchCommitment,
        frost_signature: ethexe_common::crypto::frost::SignAggregate,
    ) -> Result<ValidatorState> {
        let cloned_committer = ctx.core.committer.clone_boxed();

        tracing::info!(
            block_hash = %batch.block_hash,
            "📤 Submitting batch commitment with FROST threshold signature"
        );

        ctx.tasks.push(
            async move {
                let block_hash = batch.block_hash;
                let batch_digest = batch.to_digest();
                // Submit with FROST signature
                let event = match cloned_committer.commit_frost(batch, frost_signature.signature96).await {
                    Ok(tx) => CommitmentSubmitted {
                        block_hash,
                        batch_digest,
                        tx,
                    }.into(),
                    Err(err) => ConsensusEvent::Warning(format!(
                        "Failed to submit FROST commitment for block {block_hash}, digest {batch_digest}: {err}"
                    ))
                };
                Ok(event)
            }
            .boxed(),
        );
        Initial::create(ctx)
    }

    #[cfg(test)]
    pub fn submission(
        ctx: ValidatorContext,
        batch: BatchCommitment,
        threshold_signature: ContractSignature,
    ) -> Result<ValidatorState> {
        let cloned_committer = ctx.core.committer.clone_boxed();

        tracing::info!(
            block_hash = %batch.block_hash,
            "📤 Submitting batch commitment with ROAST threshold signature"
        );

        ctx.tasks.push(
            async move {
                let block_hash = batch.block_hash;
                let batch_digest = batch.to_digest();
                // Submit with single threshold signature
                let event = match cloned_committer.commit(batch, vec![threshold_signature]).await {
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
    use ethexe_common::{Address, ToDigest, ValidatorsVec};
    use gsigner::secp256k1::Secp256k1SignerExt;
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
        let era_index = ctx.core.timelines.era_from_ts(batch.timestamp);
        let validator_addrs: Vec<Address> = validators.clone().into();
        setup_test_dkg(
            &ctx.core.db,
            &validator_addrs,
            ctx.core.pub_key.to_address(),
            ctx.core.signatures_threshold as u16,
            era_index,
        )
        .unwrap();

        let coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert!(coordinator.is_coordinator());
        // With ROAST, coordinator sends BroadcastValidatorMessage instead of PublishMessage
        assert!(
            !coordinator.context().output.is_empty(),
            "Expected ROAST messages to be sent"
        );
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
        let era_index = ctx.core.timelines.era_from_ts(batch.timestamp);
        let validator_addrs: Vec<Address> = validators.clone().into();
        setup_test_dkg(
            &ctx.core.db,
            &validator_addrs,
            ctx.core.pub_key.to_address(),
            ctx.core.signatures_threshold as u16,
            era_index,
        )
        .unwrap();

        let reply1 = ctx
            .core
            .signer
            .validation_reply(keys[0], ctx.core.router_address, digest);

        let coordinator = Coordinator::create(ctx, validators.into(), batch).unwrap();
        assert!(coordinator.is_coordinator());

        // With ROAST, validation replies are ignored
        let coordinator = coordinator.process_validation_reply(reply1).unwrap();
        assert!(coordinator.is_coordinator());

        // Coordinator should still be waiting for ROAST signature
        // (validation replies don't affect ROAST flow)
    }

    #[test]
    fn submission_transitions_to_initial() {
        let (ctx, _, _) = mock_validator_context();
        let batch = BatchCommitment::default();
        let digest = batch.to_digest();
        let signature = ctx
            .core
            .signer
            .sign_for_contract_digest(ctx.core.router_address, ctx.core.pub_key, &digest)
            .unwrap();

        let state = Coordinator::submission(ctx, batch, signature).unwrap();
        assert!(state.is_initial());
    }
}
