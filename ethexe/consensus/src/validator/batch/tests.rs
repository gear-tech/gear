// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use std::collections::VecDeque;

use super::types::{ValidationRejectReason, ValidationStatus};

use crate::{
    mock::*,
    validator::{
        batch::{BatchLimits, types::BatchParts},
        mock::*,
    },
};

use ethexe_common::{
    Address, Announce, Digest, HashOf, SimpleBlockData, ValidatorsVec,
    consensus::{BatchCommitmentValidationRequest, DEFAULT_BATCH_SIZE_LIMIT},
    db::*,
    gear::{ChainCommitment, CodeCommitment},
    mock::*,
};
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{CodeId, H256};
use gsigner::ToDigest;

fn unwrap_rejected_reason(status: ValidationStatus) -> ValidationRejectReason {
    match status {
        ValidationStatus::Rejected { reason, .. } => reason,
        ValidationStatus::Accepted(digest) => {
            panic!(
                "Expected rejection, but got acceptance with digest {:?}",
                digest
            )
        }
    }
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_empty_batch_request() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let mut batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    batch.code_commitments = Vec::new();
    let mut request = BatchCommitmentValidationRequest::new(&batch);
    request.head = None;

    let mut announce_hash = batch.chain_commitment.clone().unwrap().head_announce;
    // Nullify the codes in database
    ctx.core
        .db
        .mutate_block_meta(block.hash, |meta| meta.codes_queue = Some(VecDeque::new()));

    // Nullify the transitions in database
    for _ in 0..2 {
        announce_hash = ctx.core.db.announce(announce_hash).unwrap().parent;
        ctx.core.db.set_announce_outcome(announce_hash, Vec::new());
    }

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::EmptyBatch
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_duplicate_code_ids() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let mut batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let duplicate = batch.code_commitments[0].clone();
    batch.code_commitments.push(duplicate);

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(
            SimpleBlockData::mock(()),
            BatchCommitmentValidationRequest::new(&batch),
        )
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::CodesHasDuplicates
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_not_waiting_code_ids() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    let mut request = BatchCommitmentValidationRequest::new(&batch);

    let missing_code = H256::random().into();
    request.codes.push(missing_code);

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::CodeNotWaitingForCommitment(missing_code)
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_non_best_chain_head() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    let best_head = batch.chain_commitment.clone().unwrap().head_announce;
    let wrong_announce = Announce::mock(block.hash);
    let wrong_head = ctx.core.db.set_announce(wrong_announce);
    ctx.core
        .db
        .mutate_announce_meta(wrong_head, |meta| meta.computed = true);

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    request.head = Some(wrong_head);

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::HeadAnnounceIsNotFromBestChain {
            requested: wrong_head,
            best: best_head,
        }
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_when_best_head_chain_is_invalid() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    let request = BatchCommitmentValidationRequest::new(&batch);
    let head = request.head.expect("expect head");

    ctx.core.db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_announce = Some(HashOf::random());
    });

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::BestHeadAnnounceChainInvalid(head)
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_digest_mismatch() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    let original_digest = request.digest;
    let mut wrong_digest = original_digest;
    while wrong_digest == original_digest {
        wrong_digest = Digest::random();
    }
    request.digest = wrong_digest;

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::BatchDigestMismatch {
            expected: wrong_digest,
            found: original_digest,
        }
    );
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn rejects_code_not_processed_yet() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let code = b"1234";
    let code_id = CodeId::generate(code);
    let chain = BlockChain::mock(10)
        .tap_mut(|chain| {
            chain.blocks[10]
                .assert_prepared_mut()
                .codes_queue
                .push_front(code_id);
            chain.codes.insert(
                code_id,
                CodeData {
                    original_bytes: code.to_vec(),
                    blob_info: Default::default(),
                    instrumented: None,
                },
            );
        })
        .setup(&ctx.core.db);
    let block = chain.blocks[10].to_simple();
    let code_commitments = vec![CodeCommitment {
        id: code_id,
        valid: true,
    }];
    let batch_parts = BatchParts {
        chain_commitment: None,
        code_commitments,
        rewards_commitment: None,
        validators_commitment: None,
    };
    let batch = crate::validator::batch::utils::create_batch_commitment(
        &ctx.core.db,
        &block,
        batch_parts,
        100,
    )
    .unwrap()
    .unwrap();

    let request = BatchCommitmentValidationRequest::new(&batch);
    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::CodeIsNotProcessedYet(code_id)
    );
}

#[tokio::test]
async fn rejects_batch_commitment_size_limit_exceeded() {
    gear_utils::init_default_logger();
    const BLOCKCHAIN_LEN: usize = 30;

    let (mut ctx, _, _) = mock_validator_context();

    // Preparing transitions for announces chain.
    let mut blockchain = BlockChain::mock(BLOCKCHAIN_LEN as u32);
    for i in 0..BLOCKCHAIN_LEN {
        blockchain.block_top_announce_mut(i).tap_mut(|announce| {
            let transitions = (0..5)
                .flat_map(|_| {
                    let commitment = ChainCommitment::mock(announce.announce.to_hash());
                    commitment.transitions
                })
                .collect::<Vec<_>>();
            announce.as_computed_mut().outcome = transitions;
        });
    }
    let blockchain = blockchain.setup(&ctx.core.db);
    let announce = blockchain
        .block_top_announce(BLOCKCHAIN_LEN - 1)
        .clone()
        .announce;
    let block = blockchain.blocks[BLOCKCHAIN_LEN - 1].to_simple();

    let batch = ctx
        .core
        .batch_manager
        .clone()
        .create_batch_commitment(block, announce.to_hash())
        .await
        .unwrap()
        .unwrap();

    {
        // Batch is correct, expecting successful ValidationStatus
        let expected_digest = batch.to_digest();
        let request = BatchCommitmentValidationRequest::new(&batch);
        let status = ctx
            .core
            .batch_manager
            .clone()
            .validate_batch_commitment(block, request)
            .await
            .unwrap();

        assert_eq!(status, ValidationStatus::Accepted(expected_digest));
    }

    {
        // Rebuilding batch with higher size_limits.
        let new_limits = BatchLimits {
            batch_size_limit: DEFAULT_BATCH_SIZE_LIMIT + 10_000_000,
            ..Default::default()
        };
        let previous_limits = ctx.core.batch_manager.replace_limits(new_limits);

        let batch = ctx
            .core
            .batch_manager
            .clone()
            .create_batch_commitment(block, announce.to_hash())
            .await
            .unwrap()
            .unwrap();

        // Set previous limits for validation.
        ctx.core.batch_manager.replace_limits(previous_limits);

        let request = BatchCommitmentValidationRequest::new(&batch);
        let status = ctx
            .core
            .batch_manager
            .clone()
            .validate_batch_commitment(block, request)
            .await
            .unwrap();
        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::BatchSizeLimitExceeded
        )
    }
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn accepts_matching_request() {
    gear_utils::init_default_logger();

    let (ctx, _, _) = mock_validator_context();
    let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
    let block = ctx.core.db.simple_block_data(batch.block_hash);

    let request = BatchCommitmentValidationRequest::new(&batch);
    let expected_digest = request.digest;

    let status = ctx
        .core
        .batch_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    match status {
        ValidationStatus::Accepted(digest) => assert_eq!(digest, expected_digest),
        ValidationStatus::Rejected { reason, .. } => {
            panic!("Expected acceptance, got rejection: {reason:?}")
        }
    }
}

#[tokio::test]
#[ntest::timeout(3000)]
async fn test_aggregate_validators_commitment() {
    gear_utils::init_default_logger();

    let (ctx, _, eth) = mock_validator_context();
    let chain = BlockChain::mock(20)
        .tap_mut(|chain| {
            chain.config.timelines.era = 10 * chain.config.timelines.slot;
            chain.config.timelines.election = 5 * chain.config.timelines.slot;
        })
        .setup(&ctx.core.db);

    let validators1: ValidatorsVec = [Address([1; 20]), Address([2; 20]), Address([3; 20])]
        .into_iter()
        .collect();
    let validators2: ValidatorsVec = [Address([4; 20]), Address([5; 20]), Address([6; 20])]
        .into_iter()
        .collect();
    eth.predefined_election_at.write().await.insert(
        chain.config.timelines.era_election_start_ts(0),
        validators1.clone(),
    );
    eth.predefined_election_at.write().await.insert(
        chain.config.timelines.era_election_start_ts(1),
        validators2.clone(),
    );

    // Before election
    let commitment = ctx
        .core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[4].to_simple())
        .await
        .unwrap();
    assert!(commitment.is_none());

    // Right at election start
    let commitment = ctx
        .core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[5].to_simple())
        .await
        .unwrap()
        .expect("Validators commitment expected");
    assert_eq!(commitment.validators, validators1);
    assert_eq!(commitment.era_index, 1);

    // Inside election period
    let commitment = ctx
        .core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[7].to_simple())
        .await
        .unwrap()
        .expect("Validators commitment expected");
    assert_eq!(commitment.validators, validators1);
    assert_eq!(commitment.era_index, 1);

    // Inside election period validators already committed
    ctx.core.db.mutate_block_meta(chain.blocks[7].hash, |meta| {
        meta.latest_era_validators_committed = 1;
    });
    let commitment = ctx
        .core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[7].to_simple())
        .await
        .unwrap();
    assert!(commitment.is_none());

    // Election for era 2 but validators are not committed for era 1
    ctx.core
        .db
        .mutate_block_meta(chain.blocks[15].hash, |meta| {
            meta.latest_era_validators_committed = 0;
        });
    let commitment = ctx
        .core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[15].to_simple())
        .await
        .unwrap()
        .expect("Validators commitment expected");
    assert_eq!(commitment.validators, validators2);
    assert_eq!(commitment.era_index, 2);

    // Election for era 2 but validators for era 3 are already committed
    ctx.core
        .db
        .mutate_block_meta(chain.blocks[15].hash, |meta| {
            meta.latest_era_validators_committed = 3;
        });
    ctx.core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[15].to_simple())
        .await
        .unwrap_err();
}
