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
    DefaultProcessing, PendingEvent, StateHandler, ValidatorContext, ValidatorState,
    initial::Initial,
};
use crate::{
    utils, validator::MAX_CHAIN_DEEPNESS, BatchCommitmentValidationReply, BatchCommitmentValidationRequest, ConsensusEvent, SignedValidationRequest
};
use anyhow::{Result, anyhow, ensure};
use derive_more::{Debug, Display};
use ethexe_common::{Address, SimpleBlockData, ToDigest, db::BlockMetaStorageRead};
use std::collections::HashSet;

/// [`Participant`] is a state of the validator that processes validation requests,
/// which are sent by the current block producer (from the coordinator state).
/// After replying to the request, it switches back to the [`Initial`] state
/// and waits for the next block.
#[derive(Debug, Display)]
#[display("PARTICIPANT")]
pub struct Participant {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    producer: Address,
}

impl StateHandler for Participant {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_request(
        self,
        request: SignedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.producer {
            self.process_validation_request(request.into_parts().0)
        } else {
            DefaultProcessing::validation_request(self, request)
        }
    }
}

impl Participant {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
    ) -> Result<ValidatorState> {
        let mut earlier_validation_request = None;
        ctx.pending_events.retain(|event| match event {
            PendingEvent::ValidationRequest(signed_data)
                if earlier_validation_request.is_none() && signed_data.address() == producer =>
            {
                earlier_validation_request = Some(signed_data.data().clone());

                false
            }
            _ => {
                // NOTE: keep all other events in queue.
                true
            }
        });

        let participant = Self {
            ctx,
            block,
            producer,
        };

        let Some(validation_request) = earlier_validation_request else {
            return Ok(participant.into());
        };

        participant.process_validation_request(validation_request)
    }

    fn process_validation_request(
        mut self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidatorState> {
        match self.process_validation_request_inner(request) {
            Ok(reply) => self.output(ConsensusEvent::PublishValidationReply(reply)),
            Err(err) => self.warning(format!("reject validation request: {err}")),
        }

        // NOTE: In both cases it returns to the initial state,
        // means - even if producer publish incorrect validation request,
        // then participant does not wait for the next validation request from producer.
        Initial::create(self.ctx)
    }

    fn process_validation_request_inner(
        &self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<BatchCommitmentValidationReply> {
        let BatchCommitmentValidationRequest {
            digest,
            head,
            codes,
        } = request;

        ensure!(
            !(head.is_none() && codes.is_empty()),
            "Empty batch (change when other commitments are supported)"
        );

        ensure!(
            !utils::has_duplicates(codes.as_slice()),
            "Duplicate codes in validation request"
        );

        // Check requested codes wait for commitment
        let waiting_codes = self
            .ctx
            .db
            .block_codes_queue(self.block.hash)
            .ok_or_else(|| {
                anyhow!(
                    "Cannot get from db block codes queue for block {}",
                    self.block.hash
                )
            })?
            .into_iter()
            .collect::<HashSet<_>>();
        ensure!(
            codes.iter().all(|code| waiting_codes.contains(code)),
            "Not all requested codes are waiting for commitment"
        );

        let chain_commitment = if let Some(head) = head {
            // TODO #4791: support head != current block hash, have to check head is predecessor of current block
            ensure!(
                head == self.block.hash,
                "Head cannot be different from current block hash"
            );

            utils::aggregate_chain_commitment(&self.ctx.db, head, true, Some(MAX_CHAIN_DEEPNESS))?
                .map(|(commitment, _)| commitment)
        } else {
            None
        };

        let code_commitments = utils::aggregate_code_commitments(&self.ctx.db, codes, true)?;
        let batch = utils::create_batch_commitment(
            &self.ctx.db,
            &self.block,
            chain_commitment,
            code_commitments,
        )?
        .ok_or_else(|| anyhow!("Batch commitment is empty for current block"))?;

        if batch.to_digest() != digest {
            return Err(anyhow!(
                "Requested and local batch commitment digests mismatch"
            ));
        }

        self.ctx
            .signer
            .sign_for_contract(self.ctx.router_address, self.ctx.pub_key, digest)
            .map(|signature| BatchCommitmentValidationReply { digest, signature })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        utils::{SignedProducerBlock, SignedValidationRequest},
        validator::mock::*,
    };
    use ethexe_common::{Digest, gear::CodeCommitment};
    use gprimitives::H256;

    #[test]
    fn create() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random());

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();

        assert!(participant.is_participant());
        assert_eq!(participant.context().pending_events.len(), 0);
    }

    #[test]
    fn create_with_pending_events() {
        let (mut ctx, keys) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = SimpleBlockData::mock(H256::random());

        // Validation request from alice - must be kept
        ctx.pending(SignedValidationRequest::mock((
            ctx.signer.clone(),
            alice,
            (),
        )));

        // Reply from producer - must be removed and processed
        ctx.pending(SignedValidationRequest::mock((
            ctx.signer.clone(),
            producer,
            (),
        )));

        // Block from producer - must be kept
        ctx.pending(SignedProducerBlock::mock((
            ctx.signer.clone(),
            producer,
            H256::random(),
        )));

        // Block from alice - must be kept
        ctx.pending(SignedProducerBlock::mock((
            ctx.signer.clone(),
            alice,
            H256::random(),
        )));

        let initial = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(initial.is_initial());

        let ctx = initial.into_context();
        assert_eq!(ctx.pending_events.len(), 3);
        assert!(matches!(
            ctx.pending_events[0],
            PendingEvent::ProducerBlock(_)
        ));
        assert!(matches!(
            ctx.pending_events[1],
            PendingEvent::ProducerBlock(_)
        ));
        assert!(matches!(
            ctx.pending_events[2],
            PendingEvent::ValidationRequest(_)
        ));

        // Pending validation request from producer was found and rejected
        assert_eq!(ctx.output.len(), 1);
        assert!(matches!(ctx.output[0], ConsensusEvent::Warning(_)));
    }

    #[test]
    fn process_validation_request_success() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.db);
        let block = simple_block_data(&ctx.db, batch.block_hash);

        let signed_request = ctx
            .signer
            .signed_data(producer, BatchCommitmentValidationRequest::new(&batch))
            .unwrap();

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());

        let ctx = initial.into_context();
        assert_eq!(ctx.output.len(), 1);

        let ConsensusEvent::PublishValidationReply(reply) = &ctx.output[0] else {
            panic!(
                "Expected PublishValidationReply event, got {:?}",
                ctx.output[0]
            );
        };
        assert_eq!(reply.digest, batch.to_digest());
        reply
            .signature
            .validate(ctx.router_address, reply.digest)
            .unwrap();
    }

    #[test]
    fn process_validation_request_failure() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random());
        let signed_request = SignedValidationRequest::mock((ctx.signer.clone(), producer, ()));

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 1);
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn test_codes_not_waiting_for_commitment() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let mut batch = prepared_mock_batch_commitment(&ctx.db);
        let block = simple_block_data(&ctx.db, batch.block_hash);

        // Add a code that's not in the waiting queue
        let extra_code = CodeCommitment::mock(());
        batch.code_commitments.push(extra_code);

        let request = BatchCommitmentValidationRequest::new(&batch);
        let signed_request = ctx.signer.signed_data(producer, request).unwrap();

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 1);
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn test_empty_codes_and_blocks() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.db, H256::random());

        // Create a request with empty blocks and codes
        let request = BatchCommitmentValidationRequest {
            digest: Digest::random(),
            head: None,
            codes: vec![],
        };

        let signed_request = ctx.signer.signed_data(producer, request).unwrap();

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 1);
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));
    }

    #[test]
    fn test_duplicate_codes_and_blocks() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.db);
        let block = simple_block_data(&ctx.db, batch.block_hash);

        // Create a request with duplicate codes
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        if !request.codes.is_empty() {
            let duplicate_code = request.codes[0];
            request.codes.push(duplicate_code);
        }

        let signed_request = ctx.signer.signed_data(producer, request).unwrap();

        let participant = Participant::create(ctx, block.clone(), producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());

        let ctx = initial.into_context();
        assert_eq!(ctx.output.len(), 1);
        assert!(matches!(ctx.output[0], ConsensusEvent::Warning(_)));
    }

    #[test]
    fn test_digest_mismatch() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.db);
        let block = simple_block_data(&ctx.db, batch.block_hash);

        // Create request with incorrect digest
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        request.digest = Digest::random(); // Set a different random digest

        let signed_request = ctx.signer.signed_data(producer, request).unwrap();

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let initial = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 1);
        assert!(matches!(
            initial.context().output[0],
            ConsensusEvent::Warning(_)
        ));
    }
}
