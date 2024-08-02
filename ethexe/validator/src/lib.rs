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

use core::hash;

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    BlockCommitment, CodeCommitment, Commitments,
};
use ethexe_network::NetworkSender;
use ethexe_sequencer::{
    AggregatedCommitments, BlockCommitmentValidationRequest, CodeCommitmentValidationRequest,
    NetworkMessage, SeqHash,
};
use ethexe_signer::{Address, PublicKey, Signature, Signer};
use gprimitives::H256;
use parity_scale_codec::Encode;
use uluru::LRUCache;

pub struct Config {
    pub pub_key: PublicKey,
    pub router_address: Address,
}

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    router_address: Address,
    signed_code_commitments: LRUCache<H256, 1000>,
    signed_block_commitments: LRUCache<H256, 100>,
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            router_address: config.router_address,
            signed_code_commitments: LRUCache::new(),
            signed_block_commitments: LRUCache::new(),
        }
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn address(&self) -> Address {
        self.pub_key.to_address()
    }

    fn aggregate<C: SeqHash>(&self, commitments: Vec<C>) -> Result<AggregatedCommitments<C>> {
        AggregatedCommitments::aggregate_commitments(
            commitments,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn aggregate_codes(
        &mut self,
        commitments: Vec<CodeCommitment>,
    ) -> Result<AggregatedCommitments<CodeCommitment>> {
        for commitment in commitments.iter() {
            self.signed_code_commitments.insert(commitment.hash());
        }
        self.aggregate(commitments)
    }

    pub fn aggregate_blocks(
        &mut self,
        commitments: Vec<BlockCommitment>,
    ) -> Result<AggregatedCommitments<BlockCommitment>> {
        for commitment in commitments.iter() {
            self.signed_block_commitments.insert(commitment.hash());
        }
        self.aggregate(commitments)
    }

    pub fn validate_code_commitments(
        &mut self,
        db: impl CodesStorage,
        requests: impl IntoIterator<Item = CodeCommitmentValidationRequest>,
    ) -> Result<Signature> {
        let mut hashes = Vec::new();
        for request in requests.into_iter() {
            if db
                .code_approved(request.code_id)
                .ok_or(anyhow!("code not found"))?
                != request.approved
            {
                return Err(anyhow!("code not approved"));
            }
            hashes.push(request.hash());
        }

        AggregatedCommitments::<CodeCommitment>::sign_commitments(
            hashes.hash(),
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
        let mut hashes = Vec::new();
        for request in requests.into_iter() {
            let BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash,
                allowed_prev_commitment_hash,
                transitions_hash,
            } = request;

            if !db
                .block_end_state_is_valid(block_hash)
                .ok_or(anyhow!("block not found"))?
            {
                return Err(anyhow!("block is not validated"));
            }

            let outcomes = db
                .block_outcome(block_hash)
                .ok_or(anyhow!("block not found"))?;
            let transitions_hash = outcomes
                .iter()
                .map(SeqHash::hash)
                .collect::<Vec<_>>()
                .hash();
            if transitions_hash != transitions_hash {
                return Err(anyhow!("block transitions hash mismatch"));
            }

            if db
                .block_prev_commitment(block_hash)
                .ok_or(anyhow!("block not found"))?
                != allowed_prev_commitment_hash
            {
                return Err(anyhow!("block prev commitment hash mismatch"));
            }

            let allowed_predecessor_block_height = db
                .block_header(allowed_pred_block_hash)
                .ok_or(anyhow!("allowed pred block not found"))?
                .height;
            let block_height = db
                .block_header(block_hash)
                .ok_or(anyhow!("block not found"))?
                .height;

            let mut block_hash = block_hash;
            (0..allowed_predecessor_block_height.saturating_sub(block_height))
                .into_iter()
                .find_map(|_| match block_hash {
                    allowed_pred_block_hash => Some(Ok(())),
                    _ => {
                        match db
                            .block_prev_commitment(block_hash)
                            .ok_or(anyhow!("block not found"))
                        {
                            Err(err) => Some(Err(err)),
                            Ok(parent) => {
                                block_hash = parent;
                                None
                            }
                        }
                    }
                })
                .ok_or(anyhow!("allowed pred block is not in correct branch"))??;

            hashes.push(request.hash());
        }

        AggregatedCommitments::<BlockCommitment>::sign_commitments(
            hashes.hash(),
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }
}
