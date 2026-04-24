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

//! Internal state of the Malachite channel-app.
//!
//! Specialized for our [`EthexeContext`]: values are [`SequencerBlock`]s,
//! not the upstream `TestContext` u64-plus-factors toy value.

use std::collections::{HashMap, VecDeque};

use anyhow::{Result, anyhow};
use bytes::Bytes;
use ethexe_common::SimpleBlockData;
use gprimitives::H256;
use tracing::{debug, error, info};

use malachitebft_app_channel::app::consensus::ProposedValue;
use malachitebft_app_channel::app::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{
    CommitCertificate, LinearTimeouts, Round, Validity, VoteExtensions,
};
use malachitebft_app_channel::app::types::{LocallyProposedValue, PeerId};

use crate::block::SequencerBlock;
use crate::codec::JsonCodec;
use crate::context::{
    Address, EthexeSigner, EthexeContext, Genesis, Height, ProposalData, ProposalFin,
    ProposalInit, ProposalPart, ValidatorSet, Value, sign_proposal_fin,
};
use crate::store::{DecidedValue, Store};
use crate::streaming::{PartStreamsMap, ProposalParts};

/// Number of historical values to keep in the store
const HISTORY_LENGTH: u64 = 1000;

/// Internal state of the Malachite channel app.
pub struct State {
    #[allow(dead_code)]
    ctx: EthexeContext,
    signing_provider: EthexeSigner,
    genesis: Genesis,
    address: Address,
    vote_extensions: HashMap<Height, VoteExtensions<EthexeContext>>,
    streams_map: PartStreamsMap,

    /// Rolling history of the most recent Ethereum chain heads the
    /// outer service has told us about. Oldest in front, newest at the
    /// back. Used to pick the *quarantine-eligible* anchor for the
    /// next sequencer block.
    pub eth_head_history: VecDeque<SimpleBlockData>,
    /// Number of Ethereum blocks behind the current head that are
    /// considered to have passed the ethexe quarantine window.
    pub quarantine_depth: u32,

    pub store: Store,
    pub current_height: Height,
    pub current_round: Round,
    pub current_proposer: Option<Address>,
}

impl State {
    pub fn new(
        ctx: EthexeContext,
        signing_provider: EthexeSigner,
        genesis: Genesis,
        address: Address,
        height: Height,
        store: Store,
        quarantine_depth: u32,
    ) -> Self {
        Self {
            ctx,
            signing_provider,
            genesis,
            current_height: height,
            current_round: Round::new(0),
            current_proposer: None,
            address,
            store,
            vote_extensions: HashMap::new(),
            streams_map: PartStreamsMap::new(),
            eth_head_history: VecDeque::new(),
            quarantine_depth,
        }
    }

    /// Append a new Ethereum chain head to the rolling history,
    /// trimming anything older than `quarantine_depth + 1` blocks.
    pub fn push_chain_head(&mut self, head: SimpleBlockData) {
        self.eth_head_history.push_back(head);
        // Keep one extra slot so we can always name a quarantine-depth
        // deep ancestor.
        let retain = (self.quarantine_depth as usize).saturating_add(1).max(1);
        while self.eth_head_history.len() > retain {
            self.eth_head_history.pop_front();
        }
    }

    /// The Ethereum block that has most recently passed the quarantine
    /// window. Returns `H256::zero()` until we've observed enough heads.
    pub fn quarantine_anchor(&self) -> H256 {
        if self.eth_head_history.is_empty() {
            return H256::zero();
        }
        // Front of the queue is always the oldest; that's what just
        // passed quarantine relative to the current tip.
        self.eth_head_history
            .front()
            .map(|b| b.hash)
            .unwrap_or_default()
    }

    pub async fn get_earliest_height(&self) -> Height {
        self.store
            .min_decided_value_height()
            .await
            .unwrap_or_default()
    }

    /// Validate an assembled proposal. TODO: real validation —
    /// currently accepts ANYTHING as the user asked for this MVP.
    pub fn validate_proposal_parts(&self, _parts: &ProposalParts) -> Result<()> {
        // TODO: proper validation:
        //   - proposer matches select_proposer(height, round)
        //   - ProposalFin signature verifies against the proposer's key
        //   - block's Transaction sequence is well-formed
        Ok(())
    }

    /// Handle an incoming proposal part; returns a `ProposedValue`
    /// once the stream completes, or `None` while still collecting.
    pub async fn received_proposal_part(
        &mut self,
        from: PeerId,
        part: StreamMessage<ProposalPart>,
    ) -> Result<Option<ProposedValue<EthexeContext>>> {
        let sequence = part.sequence;

        let Some(parts) = self.streams_map.insert(from, part) else {
            return Ok(None);
        };

        if parts.height < self.current_height {
            debug!(
                height = %self.current_height,
                part.height = %parts.height,
                part.sequence = %sequence,
                "Received outdated proposal, ignoring"
            );
            return Ok(None);
        }

        if parts.height > self.current_height {
            info!(%parts.height, %parts.round, "Buffering proposal parts for a future height");
            self.store.store_pending_proposal_parts(parts).await?;
            return Ok(None);
        }

        match self.validate_proposal_parts(&parts) {
            Ok(()) => {
                let value = Self::assemble_value_from_parts(parts)?;
                self.store.store_undecided_proposal(value.clone()).await?;
                Ok(Some(value))
            }
            Err(e) => {
                error!(error = ?e, "Rejecting invalid proposal");
                Ok(None)
            }
        }
    }

    pub async fn get_decided_value(&self, height: Height) -> Option<DecidedValue> {
        self.store.get_decided_value(height).await.ok().flatten()
    }

    pub async fn commit(
        &mut self,
        certificate: CommitCertificate<EthexeContext>,
        extensions: VoteExtensions<EthexeContext>,
    ) -> Result<()> {
        let height = certificate.height;
        let value_id = certificate.value_id;

        self.vote_extensions.insert(height.increment(), extensions);

        let proposal = self.store.get_undecided_proposal_by_value_id(value_id).await;
        let Ok(Some(proposal)) = proposal else {
            return Err(anyhow!(
                "No undecided proposal for value id {value_id} at height {height}"
            ));
        };

        self.store
            .store_decided_value(&certificate, proposal.value)
            .await?;

        let retain = Height::new(height.as_u64().saturating_sub(HISTORY_LENGTH));
        self.store.prune(height, retain).await?;

        self.current_height = self.current_height.increment();
        self.current_round = Round::Nil;
        Ok(())
    }

    pub async fn get_previously_built_value(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Option<LocallyProposedValue<EthexeContext>>> {
        let proposals = self.store.get_undecided_proposals(height, round).await?;
        assert!(proposals.len() <= 1);
        Ok(proposals
            .first()
            .map(|p| LocallyProposedValue::new(p.height, p.round, p.value.clone())))
    }

    /// Build a new proposal for `height`/`round` wrapping the given
    /// [`SequencerBlock`]. Returns the [`LocallyProposedValue`] that
    /// can be handed back to Malachite.
    pub async fn propose_value(
        &mut self,
        height: Height,
        round: Round,
        block: SequencerBlock,
    ) -> Result<LocallyProposedValue<EthexeContext>> {
        assert_eq!(height, self.current_height);

        // Accumulate extensions (from previous commit) into the new Value,
        // so vote-extension data threads through (unused for now).
        let extensions = self
            .vote_extensions
            .remove(&height)
            .unwrap_or_default()
            .extensions
            .into_iter()
            .map(|(_, e)| e.message.to_vec())
            .fold(Vec::new(), |mut acc, e| {
                acc.extend_from_slice(&e);
                acc
            });

        let value = Value {
            block,
            extensions,
        };

        let proposal = ProposedValue {
            height,
            round,
            valid_round: Round::Nil,
            proposer: self.address,
            value,
            validity: Validity::Valid,
        };
        self.store
            .store_undecided_proposal(proposal.clone())
            .await?;

        Ok(LocallyProposedValue::new(
            proposal.height,
            proposal.round,
            proposal.value,
        ))
    }

    /// Break down a [`LocallyProposedValue`] into a sequence of
    /// [`StreamMessage<ProposalPart>`] for gossip. Currently a single
    /// `Data(block)` chunk plus Init + Fin — sufficient for MVP.
    pub fn stream_proposal(
        &mut self,
        value: LocallyProposedValue<EthexeContext>,
        pol_round: Round,
    ) -> impl Iterator<Item = StreamMessage<ProposalPart>> {
        let parts = self.value_to_parts(&value, pol_round);
        let stream_id = self.stream_id(value.height, value.round);

        let mut msgs = Vec::with_capacity(parts.len() + 1);
        let mut sequence = 0;
        for part in parts {
            msgs.push(StreamMessage::new(
                stream_id.clone(),
                sequence,
                StreamContent::Data(part),
            ));
            sequence += 1;
        }
        msgs.push(StreamMessage::new(
            stream_id,
            sequence,
            StreamContent::Fin,
        ));
        msgs.into_iter()
    }

    fn stream_id(&self, height: Height, round: Round) -> StreamId {
        let mut bytes = Vec::with_capacity(12);
        bytes.extend_from_slice(&height.as_u64().to_be_bytes());
        bytes.extend_from_slice(&round.as_u32().unwrap_or_default().to_be_bytes());
        StreamId::new(bytes.into())
    }

    fn value_to_parts(
        &self,
        value: &LocallyProposedValue<EthexeContext>,
        pol_round: Round,
    ) -> Vec<ProposalPart> {
        use parity_scale_codec::Encode;

        let mut parts = Vec::with_capacity(3);

        parts.push(ProposalPart::Init(ProposalInit::new(
            value.height,
            value.round,
            pol_round,
            self.address,
        )));
        parts.push(ProposalPart::Data(ProposalData::new(value.value.block.clone())));

        // Fin signs over (height, round, data-bytes). We mirror the
        // upstream hashing strategy — keccak over the concatenation —
        // wrapped in `sign_proposal_fin` inside `context.rs`.
        let data_bytes = value.value.block.encode();
        let signature = sign_proposal_fin(
            &self.signing_provider,
            value.height,
            value.round,
            &data_bytes,
        );
        parts.push(ProposalPart::Fin(ProposalFin::new(signature)));

        parts
    }

    pub fn get_validator_set(&self, _height: Height) -> ValidatorSet {
        self.genesis.validator_set.clone()
    }

    pub fn get_timeouts(&self, _height: Height) -> LinearTimeouts {
        LinearTimeouts::default()
    }

    /// Re-assemble a [`ProposedValue`] from its streamed parts. The
    /// single `Data` part carries the whole block; Init supplies the
    /// (height, round, proposer) header.
    pub fn assemble_value_from_parts(
        parts: ProposalParts,
    ) -> Result<ProposedValue<EthexeContext>> {
        let init = parts.init().ok_or_else(|| anyhow!("Missing Init part"))?;

        let block = parts
            .parts
            .iter()
            .find_map(|p| p.as_data())
            .map(|d| d.block.clone())
            .ok_or_else(|| anyhow!("Missing Data part"))?;

        Ok(ProposedValue {
            height: parts.height,
            round: parts.round,
            valid_round: init.pol_round,
            proposer: parts.proposer,
            value: Value {
                block,
                extensions: Vec::new(),
            },
            // TODO: validate proposal signature before marking Valid.
            validity: Validity::Valid,
        })
    }
}

// ---- codec helpers used by app.rs for syncing decided values --------

pub fn encode_value(value: &Value) -> Bytes {
    <JsonCodec as Codec<Value>>::encode(&JsonCodec, value).expect("Value is serde-encodable")
}

pub fn decode_value(bytes: Bytes) -> Option<Value> {
    <JsonCodec as Codec<Value>>::decode(&JsonCodec, bytes).ok()
}
