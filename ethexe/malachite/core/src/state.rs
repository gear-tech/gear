// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Volatile per-task state for the channel-app event loop.
//!
//! Holds the runtime bookkeeping (current height/round, proposer,
//! per-peer stream reassembly) plus the handle to the persistent
//! [`Store`]. Validation, externalities callbacks, and the
//! cascade-save / cascade-finalize flows live in [`crate::app`]
//! which calls into this struct.

use std::{
    marker::PhantomData,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::{Result, anyhow};
use malachitebft_app_channel::app::{
    consensus::ProposedValue,
    streaming::{StreamContent, StreamId, StreamMessage},
    types::{
        LocallyProposedValue, PeerId,
        core::{Height as _HeightTrait, LinearTimeouts, Round, Validity},
    },
};
use malachitebft_core_types::CommitCertificate;

use crate::{
    context::{
        Height, MalachiteCtx, ProposalData, ProposalFin, ProposalInit, ProposalPart, ValidatorSet,
        Value, sign_proposal_fin,
    },
    externalities::BlockPayload,
    signing::MalachiteSigner,
    store::Store,
    streaming::{PartStreamsMap, ProposalParts},
    types::Address,
};

/// Default propose-phase deadline added on top of the proposer's own
/// build window — gives non-proposers a bit of slack so a borderline
/// slow propose doesn't trigger an unnecessary round increment.
pub(crate) const NON_PROPOSER_PROPOSE_MARGIN: Duration = Duration::from_secs(1);

/// A finalized value plus its quorum certificate — the `commit` /
/// sync data the engine asks the app for via `GetDecidedValues`.
#[derive(Clone, Debug)]
pub struct DecidedValue {
    pub value: Value,
    pub certificate: CommitCertificate<MalachiteCtx>,
}

/// Shared validator set handle — an external writer swaps the set
/// in [`Self::update`], and the next `ConsensusReady` / `Finalized`
/// reply via [`State::get_validator_set`] picks it up.
#[derive(Clone)]
pub(crate) struct SharedValidatorSet(Arc<RwLock<ValidatorSet>>);

impl SharedValidatorSet {
    pub fn new(set: ValidatorSet) -> Self {
        Self(Arc::new(RwLock::new(set)))
    }

    pub fn get(&self) -> ValidatorSet {
        self.0.read().expect("validator set lock poisoned").clone()
    }

    pub fn update(&self, set: ValidatorSet) {
        *self.0.write().expect("validator set lock poisoned") = set;
    }
}

pub(crate) struct State<P: BlockPayload> {
    pub signer: MalachiteSigner,
    pub validator_set: SharedValidatorSet,
    pub address: Address,
    pub store: Store<P>,
    streams_map: PartStreamsMap,
    pub current_height: Height,
    pub current_round: Round,
    pub current_proposer: Option<Address>,
    pub propose_timeout: Duration,
    _phantom: PhantomData<fn() -> P>,
}

impl<P: BlockPayload> State<P> {
    pub fn new(
        signer: MalachiteSigner,
        validator_set: SharedValidatorSet,
        address: Address,
        store: Store<P>,
        propose_timeout: Duration,
    ) -> Result<Self> {
        let start_height = store
            .max_finalized_height()?
            .map(|h| Height::new(h).increment())
            .unwrap_or_else(|| Height::INITIAL);
        Ok(Self {
            signer,
            validator_set,
            address,
            store,
            streams_map: PartStreamsMap::new(),
            current_height: start_height,
            current_round: Round::new(0),
            current_proposer: None,
            propose_timeout,
            _phantom: PhantomData,
        })
    }

    pub fn get_validator_set(&self, _height: Height) -> ValidatorSet {
        self.validator_set.get()
    }

    /// Round timeouts. Propose phase is bounded by the configured
    /// [`crate::MalachiteConfig::propose_timeout`] plus a small margin
    /// for non-proposers; everything else (including the per-round
    /// `propose_delta`) stays at the engine defaults.
    pub fn get_timeouts(&self, _height: Height) -> LinearTimeouts {
        LinearTimeouts {
            propose: self.propose_timeout + NON_PROPOSER_PROPOSE_MARGIN,
            ..Default::default()
        }
    }

    // ----------------------- proposal-part stream ---------------------

    /// Insert a [`StreamMessage`] from `from`. Returns
    /// `Some(parts)` once the entire stream has arrived (Init + all
    /// Data + Fin).
    pub fn ingest_proposal_part(
        &mut self,
        from: PeerId,
        part: StreamMessage<ProposalPart>,
    ) -> Option<ProposalParts> {
        self.streams_map.insert(from, part)
    }

    /// Re-assemble a [`ProposedValue`] from a completed
    /// [`ProposalParts`] sequence. The single `Data` part carries
    /// the SCALE-encoded block bytes; `Init` supplies the (height,
    /// round, proposer) header. Validation is the caller's
    /// responsibility — application-level checks happen via
    /// [`crate::Externalities::validate_block_above`] and the
    /// `ProposalFin` signature check (when wired in).
    pub fn assemble_value_from_parts(parts: ProposalParts) -> Result<ProposedValue<MalachiteCtx>> {
        let init = parts.init().ok_or_else(|| anyhow!("missing Init part"))?;
        let block_bytes = parts
            .parts
            .iter()
            .find_map(|p| p.as_data())
            .map(|d| d.block_bytes.clone())
            .ok_or_else(|| anyhow!("missing Data part"))?;
        Ok(ProposedValue {
            height: parts.height,
            round: parts.round,
            valid_round: init.pol_round,
            proposer: parts.proposer,
            value: Value::new(block_bytes),
            // Validity::Valid by default; the caller revises this if
            // its application-level check or signature check fails.
            validity: Validity::Valid,
        })
    }

    // ----------------------- propose-side helpers ---------------------

    /// Wrap a freshly-built block payload into a
    /// [`LocallyProposedValue`] for the engine. The block is
    /// SCALE-encoded once here and stays in that form on the wire.
    pub fn build_locally_proposed_value(
        &mut self,
        height: Height,
        round: Round,
        block_bytes: Vec<u8>,
    ) -> Result<LocallyProposedValue<MalachiteCtx>> {
        assert_eq!(
            height, self.current_height,
            "build_locally_proposed_value at wrong height"
        );
        let proposed = ProposedValue {
            height,
            round,
            valid_round: Round::Nil,
            proposer: self.address,
            value: Value::new(block_bytes),
            validity: Validity::Valid,
        };
        self.store.store_undecided_proposal(&proposed)?;
        Ok(LocallyProposedValue::new(
            proposed.height,
            proposed.round,
            proposed.value,
        ))
    }

    /// Reuse a prior locally-built value if the engine re-asks
    /// `GetValue` for the same `(height, round)`. Avoids wasted
    /// block-build work and prevents non-determinism (proposer might
    /// otherwise build different content the second time).
    pub fn get_previously_built_value(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Option<LocallyProposedValue<MalachiteCtx>>> {
        let proposals = self.store.get_undecided_proposals(height, round)?;
        // We only ever store our own locally-built value at our own
        // (height, round); peer values land in `received_proposal_part`
        // which assembles them via a different path.
        Ok(proposals
            .first()
            .filter(|p| p.proposer == self.address)
            .map(|p| LocallyProposedValue::new(p.height, p.round, p.value.clone())))
    }

    // ----------------------- decided / commit -------------------------

    /// Read the decided value at `height` (block + cert).
    pub fn get_decided_value(&self, height: Height) -> Option<DecidedValue> {
        let block_hash = self
            .store
            .finalized_block_at(height.as_u64())
            .ok()
            .flatten()?;
        let entry = self.store.get_block(block_hash).ok().flatten()?;
        // Pull the engine-side rich cert (with per-signer addresses)
        // from the engine-store column for sync responses.
        let cert = self
            .store
            .get_engine_certificate(height.as_u64())
            .ok()
            .flatten()?;
        let block_bytes = parity_scale_codec::Encode::encode(&entry.block());
        Some(DecidedValue {
            value: Value::new(block_bytes),
            certificate: cert,
        })
    }

    /// Commit a finalized value: pull the matching undecided proposal
    /// out of the engine store, persist the decided value + cert, and
    /// advance to the next height.
    ///
    /// Returns the committed block bytes (SCALE-encoded
    /// [`crate::Block`]) so the caller (`app.rs`) can decode it,
    /// compute the [`crate::H256`] block hash, and insert into the
    /// [`crate::store::BlockEntry`] layer.
    pub fn commit(
        &mut self,
        certificate: CommitCertificate<MalachiteCtx>,
    ) -> Result<(Vec<u8>, CommitCertificate<MalachiteCtx>)> {
        let height = certificate.height;
        let value_id = certificate.value_id;

        let proposal = self
            .store
            .get_undecided_proposal_by_value_id(&value_id)?
            .ok_or_else(|| {
                anyhow!("no undecided proposal for value id {value_id} at height {height}")
            })?;
        let block_bytes = proposal.value.block_bytes.clone();

        // Persist the engine-side certificate so future sync responses
        // can reconstruct the decided value.
        self.store
            .store_engine_certificate(height.as_u64(), &certificate)?;

        // Engine-state pruning — drop stale undecided/pending parts
        // for heights we'll never revisit.
        self.store.prune_engine_state(height.as_u64())?;

        self.current_height = self.current_height.increment();
        self.current_round = Round::Nil;
        Ok((block_bytes, certificate))
    }

    // ----------------------- streaming helpers ------------------------

    /// Break a [`LocallyProposedValue`] into a sequence of
    /// [`StreamMessage<ProposalPart>`] for gossip.
    pub fn stream_proposal(
        &mut self,
        value: LocallyProposedValue<MalachiteCtx>,
        pol_round: Round,
    ) -> impl Iterator<Item = StreamMessage<ProposalPart>> {
        let parts = self.value_to_parts(&value, pol_round);
        let stream_id = self.stream_id(value.height, value.round);
        let mut msgs = Vec::with_capacity(parts.len() + 1);
        let mut sequence = 0u64;
        for part in parts {
            msgs.push(StreamMessage::new(
                stream_id.clone(),
                sequence,
                StreamContent::Data(part),
            ));
            sequence += 1;
        }
        msgs.push(StreamMessage::new(stream_id, sequence, StreamContent::Fin));
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
        value: &LocallyProposedValue<MalachiteCtx>,
        pol_round: Round,
    ) -> Vec<ProposalPart> {
        let mut parts = Vec::with_capacity(3);
        parts.push(ProposalPart::Init(ProposalInit::new(
            value.height,
            value.round,
            pol_round,
            self.address,
        )));
        parts.push(ProposalPart::Data(ProposalData::new(
            value.value.block_bytes.clone(),
        )));
        let signature = sign_proposal_fin(
            &self.signer,
            value.height,
            value.round,
            &value.value.block_bytes,
        );
        parts.push(ProposalPart::Fin(ProposalFin::new(signature)));
        parts
    }
}
