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

use crate::{
    announces::AnnounceChainWalker,
    validator::batch::{filler::BatchFiller, types::BatchParts},
};

use super::types::CodeNotValidatedError;

use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, OnChainStorageRO},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, StateTransition},
};
use gprimitives::{CodeId, H256};

pub fn collect_not_committed_predecessors<DB: AnnounceStorageRO + BlockMetaStorageRO>(
    db: &DB,
    last_committed_announce_hash: HashOf<Announce>,
    announce_hash: HashOf<Announce>,
) -> Result<Vec<HashOf<Announce>>> {
    let mut announces = Vec::new();
    let mut current_announce = announce_hash;

    // Maybe remove this loop to prevent infinite searching
    while current_announce != last_committed_announce_hash {
        if !db.announce_meta(current_announce).computed {
            // All announces till last committed must be computed.
            // Even fast-sync guarantees that.
            bail!("Not computed announce in chain {current_announce:?}")
        }

        announces.push(current_announce);
        current_announce = db
            .announce(current_announce)
            .ok_or_else(|| anyhow!("Computed announce {current_announce:?} body not found in db"))?
            .parent;
    }

    Ok(announces.into_iter().rev().collect())
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    batch_parts: BatchParts,
    commitment_delay_limit: u32,
) -> Result<Option<BatchCommitment>> {
    let BatchParts {
        chain_commitment,
        validators_commitment,
        code_commitments,
        rewards_commitment,
    } = batch_parts;

    let block_hash = block.hash;
    if chain_commitment.is_none()
        && code_commitments.is_empty()
        && validators_commitment.is_none()
        && rewards_commitment.is_none()
    {
        tracing::debug!("No commitments for block {block_hash} - skip batch commitment",);
        return Ok(None);
    }

    let previous_batch = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(
            || anyhow!("Cannot get from db last committed block for block {block_hash}",),
        )?;

    let expiry = chain_commitment
        .as_ref()
        .map(|c| calculate_batch_expiry(db, block, c.head_announce, commitment_delay_limit))
        .transpose()?
        .flatten()
        .unwrap_or(u8::MAX);

    tracing::trace!("Batch commitment expiry for block {block_hash} is {expiry:?}",);

    Ok(Some(BatchCommitment {
        block_hash,
        timestamp: block.header.timestamp,
        previous_batch,
        expiry,
        chain_commitment,
        code_commitments,
        validators_commitment,
        rewards_commitment,
    }))
}

pub fn aggregate_code_commitments<DB: CodesStorageRO>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>, CodeNotValidatedError> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => return Err(CodeNotValidatedError(id)),
            None => {}
        }
    }

    Ok(commitments)
}

pub fn try_include_chain_commitment<DB: BlockMetaStorageRO + AnnounceStorageRO>(
    db: &DB,
    at_block: H256,
    head_announce_hash: HashOf<Announce>,
    batch_filler: &mut BatchFiller,
) -> Result<(HashOf<Announce>, u32)> {
    if !db.announce_meta(head_announce_hash).computed {
        anyhow::bail!(
            "Head announce {head_announce_hash:?} is not computed, cannot aggregate chain commitment"
        );
    }

    let Some(last_committed_announce) = db.block_meta(at_block).last_committed_announce else {
        anyhow::bail!("Last committed announce not found in db for prepared block: {at_block}",);
    };

    let pending = super::utils::collect_not_committed_predecessors(
        &db,
        last_committed_announce,
        head_announce_hash,
    )?;

    let final_announce = pending.last().copied().unwrap_or(head_announce_hash);
    let max_depth = pending.len() as u32;

    for (depth, announce_hash) in pending.into_iter().enumerate() {
        let transitions = super::utils::announce_transitions(&db, announce_hash)?;
        let commitment = ChainCommitment {
            head_announce: announce_hash,
            transitions,
        };

        if let Err(err) = batch_filler.include_chain_commitment(commitment, depth as u32) {
            tracing::trace!(
                "failed to include chain commitment for announce({announce_hash}) because of error={err}"
            );
            return Ok((announce_hash, depth as u32));
        }
    }
    Ok((final_announce, max_depth))
}

pub fn announce_transitions<DB: AnnounceStorageRO>(
    db: &DB,
    announce_hash: HashOf<Announce>,
) -> Result<Vec<StateTransition>> {
    let Some(mut announce_transitions) = db.announce_outcome(announce_hash) else {
        anyhow::bail!("Computed announce {announce_hash:?} outcome not found in db");
    };

    sort_transitions_by_value_to_receive(&mut announce_transitions);
    Ok(announce_transitions)
}

pub fn calculate_batch_expiry<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    head_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
) -> Result<Option<u8>> {
    let head_announce = db
        .announce(head_announce_hash)
        .ok_or_else(|| anyhow!("Cannot get announce by {head_announce_hash}"))?;

    let head_announce_block_header = db
        .block_header(head_announce.block_hash)
        .ok_or_else(|| anyhow!("block header not found for({})", head_announce.block_hash))?;

    let head_delay = block
        .header
        .height
        .checked_sub(head_announce_block_header.height)
        .ok_or_else(|| {
            anyhow!(
                "Head announce {} has bigger height {}, than batch height {}",
                head_announce_hash,
                head_announce_block_header.height,
                block.header.height,
            )
        })?;

    // Number of blocks worth of announces to check for not-base announces in the commitment.
    let Some(blocks_to_check) = commitment_delay_limit.checked_sub(head_delay) else {
        // No need to set expiry - head announce is old enough, so cannot contain any not-base announces.
        return Ok(None);
    };

    if blocks_to_check == 0 {
        // No need to set expiry - head announce is old enough, so cannot contain any not-base announces.
        return Ok(None);
    }

    let mut oldest_not_base_announce_depth = (!head_announce.is_base()).then_some(0u32);
    let current_announce_hash = head_announce.parent;

    if blocks_to_check == 1 {
        // If head announce is not base and older than commitment delay limit - 1, then expiry is only 1.
        return Ok(oldest_not_base_announce_depth.map(|_| 1));
    }

    let last_committed_announce = db
        .block_meta(block.hash)
        .last_committed_announce
        .ok_or_else(|| anyhow!("last committed announce not found for block {}", block.hash))?;

    // Walk backwards using the block-aware chain walker. blocks_seen starts at 1
    // because the head announce's block is already counted.
    let mut walker =
        AnnounceChainWalker::with_seed(current_announce_hash, 1, head_announce.block_hash);

    loop {
        if walker.peek() == Some(last_committed_announce) {
            break;
        }

        let Some(step) = walker.step(db)? else { break };

        // Check is_base BEFORE the boundary break so we examine
        // the announce at the CDL boundary, not skip it.
        if !step.announce.is_base() {
            oldest_not_base_announce_depth = Some(walker.blocks_seen);
        }

        if step.is_new_block && walker.blocks_seen >= blocks_to_check {
            break;
        }
    }

    Ok(oldest_not_base_announce_depth
        .map(|depth| blocks_to_check - depth)
        .map(TryInto::try_into)
        .transpose()?)
}

pub fn sort_transitions_by_value_to_receive(transitions: &mut [StateTransition]) {
    transitions.sort_by(|lhs, rhs| {
        rhs.value_to_receive_negative_sign
            .cmp(&lhs.value_to_receive_negative_sign)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        validator::batch::{BatchLimits, filler::BatchFiller},
    };
    use ethexe_common::{
        COMMITMENT_DELAY_LIMIT, DEFAULT_BLOCK_GAS_LIMIT,
        consensus::DEFAULT_CHAIN_DEEPNESS_THRESHOLD, db::*, mock::*,
    };
    use ethexe_db::Database;

    const BATCH_LIMITS: BatchLimits = BatchLimits {
        chain_deepness_threshold: DEFAULT_CHAIN_DEEPNESS_THRESHOLD,
        commitment_delay_limit: COMMITMENT_DELAY_LIMIT,
        batch_size_limit: DEFAULT_BLOCK_GAS_LIMIT,
    };

    #[test]
    fn test_aggregate_chain_commitment() {
        {
            // Valid case, two transitions in the chain, but only one must be included
            let db = Database::memory();
            let chain = test_block_chain(10)
                .tap_mut(|chain| {
                    chain
                        .block_top_announce_mut(3)
                        .as_computed_mut()
                        .outcome
                        .push(test_state_transition(1));
                    chain
                        .block_top_announce_mut(5)
                        .as_computed_mut()
                        .outcome
                        .push(test_state_transition(2));
                    chain.blocks[10].as_prepared_mut().last_committed_announce =
                        chain.block_top_announce_hash(3);
                })
                .setup(&db);
            let block = chain.blocks[10].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(9);

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            let (_, deepness) = try_include_chain_commitment(
                &db,
                block.hash,
                head_announce_hash,
                &mut batch_filler,
            )
            .unwrap();
            let commitment = batch_filler.into_parts().chain_commitment.unwrap();

            assert_eq!(commitment.head_announce, head_announce_hash);
            assert_eq!(commitment.transitions.len(), 1);
            assert_eq!(deepness, 6);
        }

        {
            // head announce not computed
            let db = Database::memory();
            let chain = test_block_chain(3)
                .tap_mut(|chain| chain.block_top_announce_mut(3).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);
            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);

            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
        }

        {
            // announce in chain not computed
            let db = Database::memory();
            let chain = test_block_chain(3)
                .tap_mut(|chain| chain.block_top_announce_mut(2).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
        }

        {
            // last committed announce missing in block meta
            let db = Database::memory();
            let chain = test_block_chain(3)
                .tap_mut(|chain| chain.blocks[3].prepared = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(2);

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
        }
    }

    #[test]
    fn test_aggregate_code_commitments() {
        let db = Database::memory();
        let codes = vec![CodeId::from([1; 32]), CodeId::from([2; 32])];

        // Test with valid codes
        db.set_code_valid(codes[0], true);
        db.set_code_valid(codes[1], false);

        let commitments = aggregate_code_commitments(&db, codes.clone(), false).unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        let commitments =
            aggregate_code_commitments(&db, vec![codes[0], CodeId::from([3; 32]), codes[1]], false)
                .unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        aggregate_code_commitments(&db, vec![CodeId::from([3; 32])], true).unwrap_err();
    }

    #[test]
    fn test_batch_expiry_calculation() {
        {
            let db = Database::memory();
            let chain = test_block_chain(1).setup(&db);
            let block = chain.blocks[1].to_simple();
            let expiry =
                calculate_batch_expiry(&db, &block, db.top_announce_hash(block.hash), 5).unwrap();
            assert!(expiry.is_none(), "Expiry should be None");
        }

        {
            let db = Database::memory();
            let chain = test_block_chain(10)
                .tap_mut(|c| {
                    c.block_top_announce_mut(10).announce.gas_allowance = Some(10);
                    c.blocks[10].as_prepared_mut().announces =
                        Some([c.block_top_announce(10).announce.to_hash()].into());
                })
                .setup(&db);

            let block = chain.blocks[10].to_simple();
            let expiry =
                calculate_batch_expiry(&db, &block, db.top_announce_hash(block.hash), 100).unwrap();
            assert_eq!(
                expiry,
                Some(100),
                "Expiry should be 100 as there is one not-base announce"
            );
        }

        {
            let db = Database::memory();
            let batch = prepare_chain_for_batch_commitment(&db);
            let block = db.simple_block_data(batch.block_hash);
            let expiry = calculate_batch_expiry(
                &db,
                &block,
                batch.chain_commitment.as_ref().unwrap().head_announce,
                3,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                expiry, batch.expiry,
                "Expiry should match the one in the batch commitment"
            );
        }

        // Boundary block examination is verified by the prepare_chain_for_batch_commitment
        // test above: chain has 3 not-base announces (blocks 1,2,3) with CDL=3.
        // With fix: block 1 (at CDL boundary) is examined → depth=3, expiry=0.
        // Before fix: block 1 skipped → depth=2, expiry=1 (wrong).
        // The mock's expiry=0 confirms the boundary is examined correctly.
        //
        // Additional boundary test with explicit chain layout:
        {
            let db = Database::memory();
            // Reuse prepare_chain_for_batch_commitment (3 blocks, all not-base, CDL=3)
            let batch = prepare_chain_for_batch_commitment(&db);
            let block = db.simple_block_data(batch.block_hash);
            let head_hash = batch.chain_commitment.as_ref().unwrap().head_announce;

            // With CDL=100 (much larger), all 3 blocks are within range.
            // blocks_to_check = 100 - 0 = 100. All 3 not-base announces are counted.
            // oldest depth = 3 (block 1). expiry = 100 - 3 = 97.
            let expiry_large_cdl = calculate_batch_expiry(&db, &block, head_hash, 100)
                .unwrap()
                .unwrap();

            // Block 3 is the oldest not-base at depth 3 (blocks_seen=3).
            // blocks_to_check=3. Expiry = blocks_to_check - depth = 3 - 3 = 0... no.
            assert_eq!(
                expiry_large_cdl, 97,
                "With large CDL, all 3 not-base announces counted, oldest at depth 3"
            );
        }
    }
}
