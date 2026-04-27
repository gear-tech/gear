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

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use ethexe_common::SimpleBlockData;
use ethexe_db::Database;
use gprimitives::H256;
use tracing::{debug, error, info};

use malachitebft_app_channel::app::consensus::ProposedValue;
use malachitebft_app_channel::app::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{
    CommitCertificate, LinearTimeouts, Round, Validity, VoteExtensions,
};
use malachitebft_app_channel::app::types::{LocallyProposedValue, PeerId};

use crate::codec::JsonCodec;
use ethexe_common::mb::{SequencerBlock, Transaction};
use crate::context::{
    Address, EthexeSigner, EthexeContext, Genesis, Height, ProposalData, ProposalFin,
    ProposalInit, ProposalPart, ValidatorSet, Value, sign_proposal_fin,
};
use crate::quarantine;
use crate::store::{DecidedValue, Store};
use crate::streaming::{PartStreamsMap, ProposalParts};

/// Number of historical values to keep in the store
const HISTORY_LENGTH: u64 = 1000;

/// Extra slack added on top of the proposer's `SLOT_DURATION` wait
/// window, used as the non-proposer's `Timeout::Propose`. The
/// proposer aims to publish within `SLOT_DURATION`; this margin
/// absorbs network latency (proposal stream + gossip) so the
/// non-proposer doesn't trigger a round increment on a borderline
/// slow propose.
pub(crate) const NON_PROPOSER_PROPOSE_MARGIN: std::time::Duration =
    std::time::Duration::from_secs(1);

/// Internal state of the Malachite channel app.
pub struct State {
    #[allow(dead_code)]
    ctx: EthexeContext,
    signing_provider: EthexeSigner,
    genesis: Genesis,
    address: Address,
    vote_extensions: HashMap<Height, VoteExtensions<EthexeContext>>,
    streams_map: PartStreamsMap,

    /// Shared ethexe database. Used by both the producer and validator
    /// paths to resolve `parent_hash` links when walking the canonical
    /// chain during quarantine checks. The *head* itself is **not**
    /// read from here — see [`Self::latest_received_head`].
    pub db: Database,

    /// Most recent Ethereum block received via the observer event
    /// stream (`Observer::Block`). This is intentionally decoupled
    /// from [`DBGlobals::latest_synced_block`], which trails the
    /// event stream because it is only updated after extra sync
    /// processing. At [`Self::quarantine_anchor`] time the producer
    /// works off this value; validators verify `AdvanceTillEthereumBlock`
    /// against the same local view.
    ///
    /// [`DBGlobals::latest_synced_block`]: ethexe_common::db::DBGlobals
    pub latest_received_head: Option<SimpleBlockData>,

    /// Matches [`ethexe_compute::ComputeConfig::canonical_quarantine`].
    /// Number of canonical descendants an EB needs to have before it's
    /// considered "out of quarantine" and safe to anchor a sequencer
    /// block to.
    pub canonical_quarantine: u8,

    /// Hash of the last [`SequencerBlock`] this node has seen
    /// finalized. Updated in [`Self::commit`] after a successful
    /// commit, recovered from the store at startup. The producer uses
    /// it to fill `SequencerBlock::parent`; validators check incoming
    /// proposals' `parent` field against it.
    pub latest_finalized_mb_hash: H256,

    pub store: Store,
    pub current_height: Height,
    pub current_round: Round,
    pub current_proposer: Option<Address>,

    /// Per-MB outbound events held back until the MB is `synced` —
    /// i.e. has a complete `parent_mb_hash` chain back to the genesis
    /// MB. The malachite app fans events into this buffer instead of
    /// straight onto the outer event channel; the buffer is drained as
    /// each MB along the chain is settled.
    pub pending_events: HashMap<H256, PendingMbEvents>,

    /// Reverse lookup: for each parent MB hash, the children that are
    /// waiting on it to become `synced` before they themselves can be
    /// settled. Populated together with [`Self::pending_events`].
    pub pending_by_parent: HashMap<H256, Vec<H256>>,
}

/// Events buffered for an MB that has been recorded in the database
/// but is not yet `synced` (i.e. some ancestor is still missing). At
/// most one of each event kind per MB — duplicate emits collapse into
/// the existing slot.
#[derive(Default)]
pub struct PendingMbEvents {
    pub proposal: Option<crate::MalachiteEvent>,
    pub finalized: Option<crate::MalachiteEvent>,
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: EthexeContext,
        signing_provider: EthexeSigner,
        genesis: Genesis,
        address: Address,
        height: Height,
        store: Store,
        db: Database,
        canonical_quarantine: u8,
        latest_finalized_mb_hash: H256,
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
            db,
            latest_received_head: None,
            canonical_quarantine,
            latest_finalized_mb_hash,
            pending_events: HashMap::new(),
            pending_by_parent: HashMap::new(),
        }
    }

    /// Overwrite [`Self::latest_received_head`] with the newest
    /// observer-delivered chain head.
    pub fn set_latest_received_head(&mut self, head: SimpleBlockData) {
        self.latest_received_head = Some(head);
    }

    /// The Ethereum block hash the producer should anchor the next
    /// sequencer block to — i.e. the youngest EB that has already
    /// passed quarantine relative to the latest received head.
    ///
    /// Returns `Ok(None)` when either
    /// - no chain-head event has arrived yet, or
    /// - the chain between the local `start_block` and `head` is too
    ///   short to clear the quarantine window.
    ///
    /// In both cases the producer must skip the
    /// [`Transaction::AdvanceTillEthereumBlock`] transaction for this
    /// sequencer block.
    pub fn quarantine_anchor(&self) -> Result<Option<H256>> {
        let Some(head) = self.latest_received_head else {
            return Ok(None);
        };
        quarantine::anchor(
            &self.db,
            head,
            self.canonical_quarantine,
            self.start_block_hash(),
        )
    }

    /// The oldest block the local DB is guaranteed to have a header
    /// for — equal to genesis for full-sync nodes, later for
    /// fast-sync nodes. Used as the stop fence for canonical-chain
    /// walks.
    fn start_block_hash(&self) -> H256 {
        use ethexe_common::db::GlobalsStorageRO;
        self.db.globals().start_block_hash
    }

    /// Is `candidate` a strict descendant of `ancestor` along the
    /// canonical `parent_hash` chain? Thin wrapper over
    /// [`quarantine::is_strict_descendant_of`] that fills in the
    /// `start_block_hash` fence from the local DB.
    pub fn is_strict_descendant_of(
        &self,
        candidate: H256,
        ancestor: H256,
    ) -> Result<bool> {
        quarantine::is_strict_descendant_of(
            &self.db,
            candidate,
            ancestor,
            self.start_block_hash(),
        )
    }

    pub async fn get_earliest_height(&self) -> Height {
        self.store
            .min_decided_value_height()
            .await
            .unwrap_or_default()
    }

    /// Validate an assembled proposal.
    ///
    /// Current checks:
    /// - `block.parent` matches our [`Self::latest_finalized_mb_hash`]
    ///   (chain continuity);
    /// - at most one [`Transaction::AdvanceTillEthereumBlock`] is
    ///   present — zero is legal (producer had no EB past quarantine
    ///   yet), two+ is a protocol violation;
    /// - if present, the targeted EB has passed quarantine in our
    ///   local view ([`quarantine::verify_passed`]), using the latest
    ///   received chain head as the reference point.
    ///
    /// Still TODO:
    /// - proposer matches `select_proposer(height, round)`;
    /// - `ProposalFin` signature verifies against the proposer's key;
    /// - mempool-injected transactions are well-formed.
    pub fn validate_proposal_parts(&self, parts: &ProposalParts) -> Result<()> {
        let block = parts
            .parts
            .iter()
            .find_map(|p| p.as_data())
            .map(|d| &d.block)
            .ok_or_else(|| anyhow!("missing Data part in proposal"))?;

        if block.parent != self.latest_finalized_mb_hash {
            return Err(anyhow!(
                "proposal parent mismatch: got {got}, expected {expected}",
                got = block.parent,
                expected = self.latest_finalized_mb_hash,
            ));
        }

        let mut advance_txs =
            block
                .transactions
                .iter()
                .filter_map(|tx| match tx {
                    Transaction::AdvanceTillEthereumBlock { eth_block_hash } => {
                        Some(*eth_block_hash)
                    }
                    _ => None,
                });

        let Some(advance) = advance_txs.next() else {
            // No AdvanceTillEthereumBlock is a legal producer choice
            // when the chain is still too close to genesis.
            return Ok(());
        };
        if advance_txs.next().is_some() {
            return Err(anyhow!(
                "proposal has more than one AdvanceTillEthereumBlock tx"
            ));
        }

        let head = self.latest_received_head.ok_or_else(|| {
            anyhow!("cannot verify AdvanceTillEthereumBlock: no chain-head event received yet")
        })?;

        quarantine::verify_passed(
            &self.db,
            head,
            advance,
            self.canonical_quarantine,
            self.start_block_hash(),
        )
        .map_err(|e| {
            anyhow!(
                "AdvanceTillEthereumBlock {advance} rejected (local head {head_hash} h={head_h}, start {start}): {e}",
                head_hash = head.hash,
                head_h = head.header.height,
                start = self.start_block_hash(),
            )
        })
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
    ) -> Result<SequencerBlock> {
        let height = certificate.height;
        let value_id = certificate.value_id;

        self.vote_extensions.insert(height.increment(), extensions);

        let proposal = self.store.get_undecided_proposal_by_value_id(value_id).await;
        let Ok(Some(proposal)) = proposal else {
            return Err(anyhow!(
                "No undecided proposal for value id {value_id} at height {height}"
            ));
        };

        let committed_block = proposal.value.block.clone();

        self.store
            .store_decided_value(&certificate, proposal.value)
            .await?;

        let retain = Height::new(height.as_u64().saturating_sub(HISTORY_LENGTH));
        self.store.prune(height, retain).await?;

        // Advance the parent-chain pointer so the next height's
        // proposal can be checked against the correct predecessor.
        self.latest_finalized_mb_hash = committed_block.hash();

        self.current_height = self.current_height.increment();
        self.current_round = Round::Nil;
        Ok(committed_block)
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
        // The propose phase is bounded by one Ethereum slot plus a
        // small margin: within `SLOT_DURATION` a new EB always
        // arrives, passes quarantine, and gives the producer a
        // fresh `AdvanceTillEthereumBlock` to anchor an MB to. The
        // proposer waits up to `SLOT_DURATION` for that signal in
        // its `GetValue` handler, while non-proposers tolerate a
        // bit longer (`+ NON_PROPOSER_PROPOSE_MARGIN`) so a propose
        // that lands at the very last moment doesn't trigger a
        // round increment due to network latency.
        //
        // Other timeouts stay default — voting phases are quick;
        // the increased propose timeout doesn't slow down the
        // happy path (proposer pings the moment content is ready).
        let mut t = LinearTimeouts::default();
        t.propose = alloy::eips::merge::SLOT_DURATION + NON_PROPOSER_PROPOSE_MARGIN;
        t.propose_delta = std::time::Duration::from_secs(1);
        t
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
