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
    initial::Initial, DefaultProcessing, PendingEvent, StateHandler, ValidatorContext,
    ValidatorState,
};
use crate::{
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        BlockCommitmentValidationRequest,
    },
    ConsensusEvent,
};
use anyhow::{anyhow, ensure, Result};
use derive_more::{Debug, Display};
use ethexe_common::{
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    ecdsa::SignedData,
    gear::CodeCommitment,
    Address, Digest, SimpleBlockData, ToDigest,
};
use gprimitives::H256;

/// [`Participant`] is a state of the validator that processes validation requests,
/// which are sent by the current block producer (from the coordinator state).
/// After replying to the request, it switches back to the [`Initial`] state
/// and waits for the next block.
#[derive(Debug, Display)]
#[display("PARTICIPANT")]
pub struct Participant {
    ctx: ValidatorContext,
    #[allow(unused)]
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
        request: SignedData<BatchCommitmentValidationRequest>,
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
        let digest = request.to_digest();
        let BatchCommitmentValidationRequest { blocks, codes } = request;

        for code_request in codes {
            log::debug!("Receive code commitment for validation: {code_request:?}");
            Self::validate_code_commitment(&self.ctx.db, code_request)?;
        }

        for block_request in blocks {
            log::debug!("Receive block commitment for validation: {block_request:?}");
            Self::validate_block_commitment(&self.ctx.db, block_request)?;
        }

        self.ctx
            .signer
            .sign_for_contract(self.ctx.router_address, self.ctx.pub_key, digest)
            .map(|signature| BatchCommitmentValidationReply { digest, signature })
    }

    fn validate_code_commitment<DB: OnChainStorageRead + CodesStorageRead>(
        db: &DB,
        request: CodeCommitment,
    ) -> Result<()> {
        let CodeCommitment {
            id,
            timestamp,
            valid,
        } = request;

        let local_timestamp = db
            .code_blob_info(id)
            .ok_or_else(|| anyhow!("Code {id} blob info is not in storage"))?
            .timestamp;

        ensure!(
            local_timestamp == timestamp,
            "Requested and local code timestamps mismatch"
        );

        let local_valid = db
            .code_valid(id)
            .ok_or_else(|| anyhow!("Code {id} is not validated by this node"))?;

        ensure!(
            local_valid == valid,
            "Requested and local code validation results mismatch"
        );

        Ok(())
    }

    fn validate_block_commitment<DB: BlockMetaStorageRead + OnChainStorageRead>(
        db: &DB,
        request: BlockCommitmentValidationRequest,
    ) -> Result<()> {
        let BlockCommitmentValidationRequest {
            block_hash,
            block_timestamp,
            previous_non_empty_block,
            predecessor_block,
            transitions_digest,
        } = request;

        ensure!(
            db.block_computed(block_hash),
            "Requested block {block_hash} is not processed by this node"
        );

        let header = db.block_header(block_hash).ok_or_else(|| {
            anyhow!("Requested block {block_hash} header wasn't found in storage")
        })?;

        ensure!(header.timestamp == block_timestamp, "Timestamps mismatch");

        let local_outcome_digest = db
            .block_outcome(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db outcome for block {block_hash}"))?
            .iter()
            .collect::<Digest>();
        ensure!(
            local_outcome_digest == transitions_digest,
            "Requested and local transitions digests length mismatch"
        );

        let local_previous_not_empty_block =
            db.previous_not_empty_block(block_hash).ok_or_else(|| {
                anyhow!("Cannot get from db previous not empty for block {block_hash}")
            })?;
        ensure!(
            local_previous_not_empty_block == previous_non_empty_block,
            "Requested and local previous commitment block hash mismatch"
        );

        // TODO: #4579 rename max_distance and make it configurable
        ensure!(
            Self::verify_is_predecessor(db, predecessor_block, block_hash, None)?,
            "{block_hash} is not a predecessor of {predecessor_block}"
        );

        Ok(())
    }

    /// Verify whether `pred_hash` is a predecessor of `block_hash` in the chain.
    fn verify_is_predecessor(
        db: &impl OnChainStorageRead,
        block_hash: H256,
        pred_hash: H256,
        max_distance: Option<u32>,
    ) -> Result<bool> {
        if block_hash == pred_hash {
            return Ok(true);
        }

        let block_header = db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?;

        if block_header.parent_hash == pred_hash {
            return Ok(true);
        }

        let pred_height = db
            .block_header(pred_hash)
            .ok_or_else(|| anyhow!("header not found for pred block: {pred_hash}"))?
            .height;

        let distance = block_header.height.saturating_sub(pred_height);
        if max_distance.map(|d| d < distance).unwrap_or(false) {
            return Err(anyhow!("distance is too large: {distance}"));
        }

        let mut block_hash = block_hash;
        for _ in 0..=distance {
            if block_hash == pred_hash {
                return Ok(true);
            }
            block_hash = db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::{db::OnChainStorageWrite, BlockHeader};
    use ethexe_db::Database;

    #[test]
    fn create() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();

        assert!(participant.is_participant());
        assert_eq!(participant.context().pending_events.len(), 0);
    }

    #[test]
    fn create_with_pending_events() {
        let (mut ctx, keys) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = mock_simple_block_data();

        // Validation request from alice - must be kept
        ctx.pending(mock_validation_request(&ctx.signer, alice).1);

        // Reply from producer - must be removed and processed
        ctx.pending(mock_validation_request(&ctx.signer, producer).1);

        // Block from producer - must be kept
        ctx.pending(mock_producer_block(&ctx.signer, producer, H256::random()).1);

        // Block from alice - must be kept
        ctx.pending(mock_producer_block(&ctx.signer, alice, H256::random()).1);

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
        let block = mock_simple_block_data();
        let (_, signed_request) = mock_validation_request(&ctx.signer, producer);

        prepare_code_commitment(&ctx.db, signed_request.data().codes[0].clone());
        prepare_code_commitment(&ctx.db, signed_request.data().codes[1].clone());

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();
        let participant = participant
            .process_validation_request(signed_request)
            .unwrap();

        assert!(participant.is_initial());
        assert_eq!(participant.context().output.len(), 1);
        assert!(matches!(
            participant.context().output[0],
            ConsensusEvent::PublishValidationReply(_)
        ));
    }

    #[test]
    fn process_validation_request_failure() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, signed_request) = mock_validation_request(&ctx.signer, producer);

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
    fn validate_code_commitment() {
        let db = Database::memory();
        let mut code_commitment = mock_code_commitment();

        // No enough data in db
        Participant::validate_code_commitment(&db, code_commitment.clone()).unwrap_err();

        prepare_code_commitment(&db, code_commitment.clone());

        // Incorrect validation status
        code_commitment.valid = false;
        Participant::validate_code_commitment(&db, code_commitment.clone()).unwrap_err();

        // Incorrect timestamp
        code_commitment.valid = true;
        code_commitment.timestamp = 111;
        Participant::validate_code_commitment(&db, code_commitment.clone()).unwrap_err();

        code_commitment.timestamp = 123;
        Participant::validate_code_commitment(&db, code_commitment).unwrap();
    }

    #[test]
    fn validate_block_commitment() {
        let db = Database::from_one(&ethexe_db::MemDb::default());
        let (_, block_commitment) = prepare_block_commitment(
            &db,
            mock_block_commitment(H256::random(), H256::random(), H256::random()),
        );

        let request = BlockCommitmentValidationRequest::new(&block_commitment);

        Participant::validate_block_commitment(&db, request.clone()).unwrap();

        // Incorrect timestamp
        let mut incorrect_request = request.clone();
        incorrect_request.block_timestamp += 1;
        Participant::validate_block_commitment(&db, incorrect_request).unwrap_err();

        // Incorrect block hash
        let mut incorrect_request = request.clone();
        incorrect_request.predecessor_block = H256::random();
        Participant::validate_block_commitment(&db, incorrect_request).unwrap_err();

        // Incorrect previous committed block
        let mut incorrect_request = request.clone();
        incorrect_request.previous_non_empty_block = H256::random();
        Participant::validate_block_commitment(&db, incorrect_request).unwrap_err();

        // Incorrect transitions digest
        let mut incorrect_request = request.clone();
        incorrect_request.transitions_digest = Digest([2; 32]);
        Participant::validate_block_commitment(&db, incorrect_request).unwrap_err();

        // Block is not processed by this node
        let mut incorrect_request = request.clone();
        incorrect_request.block_hash = H256::random();
        Participant::validate_block_commitment(&db, incorrect_request).unwrap_err();
    }

    #[test]
    fn verify_is_predecessor() {
        let db = Database::from_one(&ethexe_db::MemDb::default());

        let blocks = [H256::random(), H256::random(), H256::random()];
        db.set_block_header(
            blocks[0],
            BlockHeader {
                height: 100,
                timestamp: 100,
                parent_hash: H256::zero(),
            },
        );
        db.set_block_header(
            blocks[1],
            BlockHeader {
                height: 101,
                timestamp: 101,
                parent_hash: blocks[0],
            },
        );
        db.set_block_header(
            blocks[2],
            BlockHeader {
                height: 102,
                timestamp: 102,
                parent_hash: blocks[1],
            },
        );

        Participant::verify_is_predecessor(&db, blocks[1], H256::random(), None)
            .expect_err("Unknown pred block is provided");

        Participant::verify_is_predecessor(&db, H256::random(), blocks[0], None)
            .expect_err("Unknown block is provided");

        Participant::verify_is_predecessor(&db, blocks[2], blocks[0], Some(1))
            .expect_err("Distance is too large");

        // Another chain block
        let block3 = H256::random();
        db.set_block_header(
            block3,
            BlockHeader {
                height: 1,
                timestamp: 1,
                parent_hash: blocks[0],
            },
        );
        Participant::verify_is_predecessor(&db, blocks[2], block3, None)
            .expect_err("Block is from other chain with incorrect height");

        assert!(Participant::verify_is_predecessor(&db, blocks[2], blocks[0], None).unwrap());
        assert!(Participant::verify_is_predecessor(&db, blocks[2], blocks[0], Some(2)).unwrap());
        assert!(!Participant::verify_is_predecessor(&db, blocks[1], blocks[2], Some(1)).unwrap());
        assert!(Participant::verify_is_predecessor(&db, blocks[1], blocks[1], None).unwrap());
    }
}
