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

use std::ops::Not;

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    BlockCommitment, CodeCommitment,
};
use ethexe_sequencer::{agro, AggregatedCommitments, BlockCommitmentValidationRequest};
use ethexe_signer::{Address, AsDigest, PublicKey, Signature, Signer};
use gprimitives::H256;

pub struct Config {
    pub pub_key: PublicKey,
    pub router_address: Address,
}

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    router_address: Address,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            router_address: config.router_address,
        }
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn address(&self) -> Address {
        self.pub_key.to_address()
    }

    fn aggregate<C: AsDigest>(&self, commitments: Vec<C>) -> Result<AggregatedCommitments<C>> {
        AggregatedCommitments::aggregate_commitments(
            commitments,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn aggregate_codes(
        &self,
        commitments: Vec<CodeCommitment>,
    ) -> Result<AggregatedCommitments<CodeCommitment>> {
        self.aggregate(commitments)
    }

    pub fn aggregate_blocks(
        &self,
        commitments: Vec<BlockCommitment>,
    ) -> Result<AggregatedCommitments<BlockCommitment>> {
        self.aggregate(commitments)
    }

    pub fn validate_code_commitments(
        &mut self,
        db: impl CodesStorage,
        requests: impl IntoIterator<Item = CodeCommitment>,
    ) -> Result<Signature> {
        let mut digests = Vec::new();
        for request in requests.into_iter() {
            if db
                .code_approved(request.code_id)
                .ok_or(anyhow!("code not found"))?
                != request.approved
            {
                return Err(anyhow!("approved mismatch"));
            }
            digests.push(request.as_digest());
        }

        agro::sign_digest(
            digests.as_digest(),
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn validate_block_commitments(
        &mut self,
        db: impl BlockMetaStorage,
        requests: impl IntoIterator<Item = BlockCommitmentValidationRequest>,
    ) -> Result<Signature> {
        let mut digests = Vec::new();
        for request in requests.into_iter() {
            let BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash,
                allowed_prev_commitment_hash,
                transitions_digest: transitions_hash,
            } = request;

            if db
                .block_end_state_is_valid(block_hash)
                .ok_or(anyhow!("block not found"))?
                .not()
            {
                return Err(anyhow!("block is not validated"));
            }

            if db
                .block_outcome(block_hash)
                .ok_or(anyhow!("block not found"))?
                .iter()
                .map(AsDigest::as_digest)
                .collect::<Vec<_>>()
                .as_digest()
                .ne(&transitions_hash)
            {
                return Err(anyhow!("block transitions hash mismatch"));
            }

            if db
                .block_prev_commitment(block_hash)
                .ok_or(anyhow!("block not found"))?
                .ne(&allowed_prev_commitment_hash)
            {
                return Err(anyhow!("block prev commitment hash mismatch"));
            }

            if Self::verify_is_predecessor(&db, allowed_pred_block_hash, block_hash, None)?.not() {
                return Err(anyhow!(
                    "{block_hash} is not a predecessor of {allowed_pred_block_hash}"
                ));
            }

            digests.push(request.as_digest());
        }

        agro::sign_digest(
            digests.as_digest(),
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    /// Verify whether `pred_hash` is a predecessor of `block_hash` in the chain.
    fn verify_is_predecessor(
        db: &impl BlockMetaStorage,
        block_hash: H256,
        pred_hash: H256,
        max_distance: Option<u32>,
    ) -> Result<bool> {
        let pred_height = db
            .block_header(pred_hash)
            .ok_or(anyhow!("header not found for pred block: {pred_hash}"))?
            .height;

        let block_height = db
            .block_header(block_hash)
            .ok_or(anyhow!("header not found for block: {block_hash}"))?
            .height;

        let distance = block_height.saturating_sub(pred_height);
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
                .ok_or(anyhow!("header not found for block: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }
}
