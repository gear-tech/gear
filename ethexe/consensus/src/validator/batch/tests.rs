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

use super::types::{ValidationRejectReason, ValidationStatus};

use crate::{
    mock::*,
    validator::{batch::types::BatchParts, mock::*},
};

use ethexe_common::{
    Address, Announce, Digest, HashOf, SimpleBlockData, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest, db::*, gear::CodeCommitment, mock::*,
};
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{CodeId, H256};

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
    let empty_request = BatchCommitmentValidationRequest {
        digest: Digest::zero(),
        head: None,
        codes: vec![],
        validators: false,
        rewards: false,
    };

    let status = ctx
        .core
        .batch_manager
        .validate(SimpleBlockData::mock(()), empty_request)
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
        .validate(
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
        .validate(block, request)
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
    let mut request = BatchCommitmentValidationRequest::new(&batch);
    let best_head = request.head.expect("chain commitment expected");

    let wrong_announce = Announce::mock(block.hash);
    let wrong_head = ctx.core.db.set_announce(wrong_announce);
    request.head = Some(wrong_head);
    ctx.core
        .db
        .mutate_announce_meta(wrong_head, |meta| meta.computed = true);

    let status = ctx
        .core
        .batch_manager
        .validate(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::HeadAnnounceIsNotBest {
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
    let best_head = request.head.expect("chain commitment expected");

    ctx.core.db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_announce = Some(HashOf::random());
    });

    let status = ctx
        .core
        .batch_manager
        .validate(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::BestHeadAnnounceChainInvalid(best_head)
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
        .validate(block, request)
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
                .as_prepared_mut()
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

    let announce = Announce::mock(block.hash);
    let announce_hash = ctx.core.db.set_announce(announce);

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    request.head = Some(announce_hash);
    let status = ctx
        .core
        .batch_manager
        .validate(block, request)
        .await
        .unwrap();

    assert_eq!(
        unwrap_rejected_reason(status),
        ValidationRejectReason::CodeIsNotProcessedYet(code_id)
    );
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
        .validate(block, request)
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

    let (mut ctx, _, eth) = mock_validator_context();
    let chain = BlockChain::mock(20)
        .tap_mut(|chain| {
            chain.config.timelines.era = 10 * chain.config.timelines.slot;
            chain.config.timelines.election = 5 * chain.config.timelines.slot;
        })
        .setup(&ctx.core.db);
    ctx.core
        .batch_manager
        .update_timelines(chain.config.timelines);

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
    ctx.core
        .db
        .set_block_validators_committed_for_era(chain.blocks[7].hash, 1);
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
        .set_block_validators_committed_for_era(chain.blocks[15].hash, 0);
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
        .set_block_validators_committed_for_era(chain.blocks[15].hash, 3);
    ctx.core
        .batch_manager
        .aggregate_validators_commitment(&chain.blocks[15].to_simple())
        .await
        .unwrap_err();
}
