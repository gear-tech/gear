// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

//! Channel-app event loop. Translates malachite [`AppMsg`]s into:
//!
//! - calls into [`crate::Externalities`] (build / validate / save /
//!   finalize),
//! - outbound [`crate::MalachiteEvent`]s to the service stream,
//! - storage operations against the [`crate::store::Store`].
//!
//! The strict ordering of `save_block` / `mark_block_as_finalized`
//! callbacks documented on [`crate::Externalities`] is enforced by
//! [`Store::cascade_save`] / [`Store::cascade_finalize`].
//!
//! Each [`AppMsg`] variant is paired with a `process_*` method on
//! [`AppMsgHandler`] that performs the work and returns the value the
//! engine expects in its reply channel. The dispatch in
//! [`AppMsgHandler::handle_app_msg`] owns the reply channel itself:
//! on `Ok` it forwards the produced value, on `Err` it logs and sends
//! a per-variant default so a transient storage error never stalls
//! the engine.

use crate::{
    codec::{decode_value, encode_value},
    context::{Height, MalachiteCtx, ProposalPart, ValueId},
    externalities::{BlockPayload, Externalities},
    state::State,
    store::BlockEntry,
    streaming::ProposalParts,
    types::{Address, Block, CommitCertificate, H256, MalachiteEvent},
};
use anyhow::{Context as _, Result, anyhow};
use bytes::Bytes;
use malachitebft_app_channel::{
    AppMsg, Channels, NetworkMsg,
    app::{
        consensus::{Role, VoteExtensionError},
        engine::host::{HeightParams, Next},
        streaming::{StreamContent, StreamMessage},
        types::{
            LocallyProposedValue, PeerId, ProposedValue,
            core::{Round, Validity, utils::height::HeightRangeExt},
            sync::RawDecidedValue,
        },
    },
};
use parity_scale_codec::{Decode, Encode};
use std::{ops::RangeInclusive, sync::Arc};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

type EngineCert = malachitebft_core_types::CommitCertificate<MalachiteCtx>;

/// Run the channel-app event loop. Terminates when the consensus
/// channel closes (engine shut down).
pub async fn run<P, EXT>(
    state: State<P>,
    channels: Channels<MalachiteCtx>,
    externalities: Arc<EXT>,
    event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
) -> Result<()>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
    AppMsgHandler {
        state,
        channels,
        externalities,
        event_tx,
    }
    .run()
    .await
}

/// Owns the channel-app event-loop state and dispatches each
/// [`AppMsg`] variant to its matching `process_*` method. The dispatch
/// holds the engine reply channel — `process_*` only produces the
/// value (or fails) and never touches the reply itself.
struct AppMsgHandler<P: BlockPayload, EXT: Externalities<P>> {
    state: State<P>,
    channels: Channels<MalachiteCtx>,
    externalities: Arc<EXT>,
    event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
}

impl<P, EXT> AppMsgHandler<P, EXT>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
    async fn run(mut self) -> Result<()> {
        loop {
            let Some(msg) = self.channels.consensus.recv().await else {
                return Err(anyhow!("consensus channel closed"));
            };
            self.handle_app_msg(msg).await?;
        }
    }

    async fn handle_app_msg(&mut self, msg: AppMsg<MalachiteCtx>) -> Result<()> {
        match msg {
            // ConsensusReady
            AppMsg::ConsensusReady { reply } => {
                if reply.send(self.process_consensus_ready()).is_err() {
                    error!("ConsensusReady: failed to send reply");
                }
            }

            // StartedRound
            AppMsg::StartedRound {
                height,
                round,
                proposer,
                role,
                reply_value,
            } => {
                info!(%height, %round, %proposer, ?role, "Started round");
                let proposals = self
                    .process_started_round(height, round, proposer, role)
                    .await
                    .unwrap_or_else(|e| {
                        error!(?e, %height, %round, "StartedRound: process failed");
                        Vec::new()
                    });
                if reply_value.send(proposals).is_err() {
                    error!("StartedRound: failed to send proposals reply");
                }
            }

            // GetValue (we are proposer)
            AppMsg::GetValue {
                height,
                round,
                timeout: _,
                reply,
            } => {
                info!(%height, %round, "GetValue");
                match self.process_get_value(height, round).await {
                    Ok(proposal) => {
                        if reply.send(proposal.clone()).is_err() {
                            error!("GetValue: failed to send proposal reply");
                        }
                        for stream_message in self.state.stream_proposal(proposal, Round::Nil) {
                            self.channels
                                .network
                                .send(NetworkMsg::PublishProposalPart(stream_message))
                                .await?;
                        }
                    }
                    Err(e) => {
                        // No usable default for `LocallyProposedValue` —
                        // dropping the reply sender lets the engine time
                        // out the propose step and advance the round.
                        error!(?e, %height, %round, "GetValue: process failed — skipping reply");
                    }
                }
            }

            // Vote extensions (unused — return defaults).
            AppMsg::ExtendVote { reply, .. } => {
                if reply.send(self.process_extend_vote()).is_err() {
                    error!("ExtendVote: failed to send reply");
                }
            }
            AppMsg::VerifyVoteExtension { reply, .. } => {
                if reply.send(self.process_verify_vote_extension()).is_err() {
                    error!("VerifyVoteExtension: failed to send reply");
                }
            }

            // ReceivedProposalPart (we are not proposer)
            AppMsg::ReceivedProposalPart { from, part, reply } => {
                let part_type = match &part.content {
                    StreamContent::Data(p) => p.get_type(),
                    StreamContent::Fin => "fin",
                };
                info!(%from, %part.sequence, part.type = %part_type, "ReceivedProposalPart");
                let value = self
                    .process_received_proposal_part(from, part)
                    .await
                    .unwrap_or_else(|e| {
                        error!(?e, "ReceivedProposalPart: process failed");
                        None
                    });
                if reply.send(value).is_err() {
                    error!("ReceivedProposalPart: failed to send reply");
                }
            }

            // Decided (info only — Finalized fires next).
            AppMsg::Decided { certificate, .. } => {
                self.process_decided(&certificate);
            }

            // Finalized (commit + cascade).
            AppMsg::Finalized {
                certificate,
                extensions: _,
                evidence,
                reply,
            } => {
                info!(
                    height = %certificate.height,
                    round = %certificate.round,
                    value = %certificate.value_id,
                    signatures = certificate.commit_signatures.len(),
                    evidence = ?evidence,
                    "Finalized"
                );
                let next = match self.process_finalized(certificate).await {
                    Ok(()) => {
                        let h = self.state.current_height;
                        Next::Start(
                            h,
                            HeightParams::new(
                                self.state.get_validator_set(h),
                                self.state.get_timeouts(h),
                                None,
                            ),
                        )
                    }
                    Err(e) => {
                        let h = self.state.current_height;
                        error!(?e, height = %h, "Finalized: commit failed — restarting height");
                        Next::Restart(
                            h,
                            HeightParams::new(
                                self.state.get_validator_set(h),
                                self.state.get_timeouts(h),
                                None,
                            ),
                        )
                    }
                };
                if reply.send(next).is_err() {
                    error!("Finalized: failed to send Next reply");
                }
            }

            // Sync path
            AppMsg::ProcessSyncedValue {
                height,
                round,
                proposer,
                value_bytes,
                reply,
            } => {
                info!(%height, %round, "ProcessSyncedValue");
                let value = self
                    .process_synced_value(height, round, proposer, value_bytes)
                    .await
                    .unwrap_or_else(|e| {
                        error!(?e, %height, %round, "ProcessSyncedValue: process failed");
                        None
                    });
                if reply.send(value).is_err() {
                    error!("ProcessSyncedValue: failed to send reply");
                }
            }

            AppMsg::GetDecidedValues { range, reply } => {
                let values = self.process_get_decided_values(range).unwrap_or_else(|e| {
                    error!(?e, "GetDecidedValues: process failed");
                    Vec::new()
                });
                if reply.send(values).is_err() {
                    error!("GetDecidedValues: failed to send reply");
                }
            }

            AppMsg::GetHistoryMinHeight { reply } => {
                let h = self.process_get_history_min_height().unwrap_or_else(|e| {
                    error!(?e, "GetHistoryMinHeight: process failed");
                    Height::default()
                });
                if reply.send(h).is_err() {
                    error!("GetHistoryMinHeight: failed to send reply");
                }
            }

            AppMsg::RestreamProposal {
                height,
                round,
                valid_round,
                address: _,
                value_id,
            } => {
                if let Err(e) = self
                    .process_restream_proposal(height, round, valid_round, value_id)
                    .await
                {
                    error!(?e, %height, %round, "RestreamProposal: process failed");
                }
            }
        }
        Ok(())
    }

    // --------------------------- processors ---------------------------

    /// Infallible: the start height was resolved at [`State::new`]
    /// and lives in `self.state.current_height`. Nothing here touches
    /// the store, so this can never fail at message-handling time.
    fn process_consensus_ready(&self) -> (Height, HeightParams<MalachiteCtx>) {
        let start_height = self.state.current_height;
        info!(%start_height, "Consensus ready");
        let params = HeightParams::new(
            self.state.get_validator_set(start_height),
            self.state.get_timeouts(start_height),
            None,
        );
        (start_height, params)
    }

    async fn process_started_round(
        &mut self,
        height: Height,
        round: Round,
        proposer: Address,
        _role: Role,
    ) -> Result<Vec<ProposedValue<MalachiteCtx>>> {
        self.state.current_height = height;
        self.state.current_round = round;
        self.state.current_proposer = Some(proposer);

        // Promote any pending parts buffered for this (height, round)
        // into proper undecided proposals.
        //
        // TODO +_+_+ : two fragility issues here, flagged 2/3 in the
        // audit iter 2.
        //
        // (2) [mitigated by this refactor] The `?` on store calls
        // below no longer kills the app task — `handle_app_msg` now
        // catches the error and sends an empty `Vec<ProposedValue>`
        // reply, so the engine proceeds rather than awaiting forever.
        // A targeted unit reproduce still needs a full engine fake;
        // TODO captures the issue with the exact line refs.
        let pending = self.state.store.get_pending_proposal_parts(height, round)?;
        for parts in pending {
            match self.assemble_and_validate(&parts).await {
                Ok(proposed) => {
                    self.state.store.store_undecided_proposal(&proposed)?;
                }
                Err(e) => {
                    error!(?e, "rejecting invalid pending proposal");
                }
            }
        }

        self.state.store.get_undecided_proposals(height, round)
    }

    async fn process_get_value(
        &mut self,
        height: Height,
        round: Round,
    ) -> Result<LocallyProposedValue<MalachiteCtx>> {
        if let Some(p) = self.state.get_previously_built_value(height, round)? {
            info!("re-using previously built value");
            return Ok(p);
        }
        // Compute parent_hash from our finalized height-1 record.
        // `H256::zero()` for genesis.
        let parent_hash = if height.as_u64() <= 1 {
            H256::zero()
        } else {
            self.state
                .store
                .finalized_block_at(height.as_u64() - 1)?
                .ok_or_else(|| {
                    anyhow!(
                        "no finalized block at height {} — Malachite invariant violated",
                        height.as_u64() - 1,
                    )
                })?
        };
        let build_fut = self.externalities.build_block_above(parent_hash);
        let payload = match tokio::time::timeout(self.state.propose_timeout, build_fut).await {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return Err(anyhow!("Externalities::build_block_above failed: {e:?}")),
            Err(_) => {
                warn!(
                    propose_timeout = ?self.state.propose_timeout,
                    "Externalities::build_block_above timed out"
                );
                return Err(anyhow!("Externalities::build_block_above timed out"));
            }
        };
        let block = Block::<P>::new(parent_hash, height.as_u64(), payload);
        let block_bytes = block.encode();
        self.state
            .build_locally_proposed_value(height, round, block_bytes)
    }

    fn process_extend_vote(&self) -> Option<Bytes> {
        None
    }

    fn process_verify_vote_extension(&self) -> Result<(), VoteExtensionError> {
        Ok(())
    }

    async fn process_received_proposal_part(
        &mut self,
        from: PeerId,
        part: StreamMessage<ProposalPart>,
    ) -> Result<Option<ProposedValue<MalachiteCtx>>> {
        let Some(parts) = self.state.ingest_proposal_part(from, part) else {
            return Ok(None);
        };
        if parts.height < self.state.current_height {
            info!(parts.height = %parts.height, "Dropping outdated proposal");
            return Ok(None);
        }
        if parts.height > self.state.current_height {
            // Buffer until the engine catches up to that height.
            //
            // TODO +_+_+ : a peer can pump completed `ProposalParts`
            // at arbitrarily large heights (`value_id` is content-
            // addressed → every junk payload is a fresh key) and the
            // rows never get pruned: `prune_engine_state` only sweeps
            // `height <= current_height`. RocksDB grows without
            // bound. Pinned by the (ignored) test
            // `store::tests::pending_proposal_parts_at_future_heights_persist_after_prune`.
            // Fix needs: height-window cap (refuse to persist beyond
            // `current_height + N`) plus per-peer rate limit. See
            // also the related validator-peer-id allowlist mitigation
            // in `streaming.rs::PartStreamsMap` TODO.
            let value_id = compute_value_id_from_parts(&parts);
            self.state
                .store
                .store_pending_proposal_parts(&parts, &value_id)?;
            return Ok(None);
        }
        let proposed = self.assemble_and_validate(&parts).await?;
        self.state.store.store_undecided_proposal(&proposed)?;
        Ok(Some(proposed))
    }

    fn process_decided(&self, certificate: &EngineCert) {
        info!(
            height = %certificate.height,
            round = %certificate.round,
            value = %certificate.value_id,
            signatures = certificate.commit_signatures.len(),
            "Decided"
        );
    }

    // TODO +_+_+ : semantic mismatch between engine and app on a
    // partial finalize.
    //
    // `state.commit()` succeeds (engine certificate persisted,
    // `prune_engine_state` ran, `state.current_height` advanced to
    // h+1) but `ingest_finalized` then fails partway through
    // `cascade_save` / `cascade_finalize`. We swallow the error and
    // return `Ok(())`, so the dispatch in `handle_app_msg` sends
    // `Next::Start(h+1, ...)` — the engine moves forward.
    //
    // Result: the application side (`Externalities::save_block` /
    // `mark_block_as_finalized`) never observed the block, but
    // `BlockEntry` may sit in the store with `saved=false,
    // finalized=false`; engine certificates exist for heights the app
    // has not been told about. From the engine's perspective height h
    // is fully decided; from the app's perspective the cascade is
    // half-applied and no `BlockFinalized` event was emitted.
    //
    // Fix needs splitting `state.commit()` into a read-only phase
    // (pull undecided proposal + decode block_bytes) and a write
    // phase (persist engine_certificate, prune, advance current_height
    // / current_round), with `ingest_finalized` in between. If
    // `ingest_finalized` fails, abort before the write phase, return
    // Err, and let the dispatch send `Next::Restart(h, ...)`. The
    // engine then retries the same height with consistent state on
    // both sides.
    async fn process_finalized(&mut self, certificate: EngineCert) -> Result<()> {
        let (block_bytes, _cert) = self.state.commit(certificate.clone())?;
        if let Err(e) = self.ingest_finalized(certificate, block_bytes).await {
            error!(?e, "ingest_finalized failed");
            let _ = self.event_tx.send(Err(e));
        }
        Ok(())
    }

    async fn process_synced_value(
        &mut self,
        height: Height,
        round: Round,
        proposer: Address,
        value_bytes: Bytes,
    ) -> Result<Option<ProposedValue<MalachiteCtx>>> {
        let parsed = decode_value(value_bytes).map(|v| ProposedValue {
            height,
            round,
            valid_round: Round::Nil,
            proposer,
            value: v,
            validity: Validity::Valid,
        });
        if let Some(ref proposed) = parsed {
            self.state.store.store_undecided_proposal(proposed)?;
        }
        Ok(parsed)
    }

    fn process_get_decided_values(
        &self,
        range: RangeInclusive<Height>,
    ) -> Result<Vec<RawDecidedValue<MalachiteCtx>>> {
        let mut values = Vec::new();
        for height in range.iter_heights() {
            if let Some(dv) = self.state.get_decided_value(height) {
                values.push(RawDecidedValue {
                    certificate: dv.certificate,
                    value_bytes: encode_value(&dv.value),
                });
            }
        }
        Ok(values)
    }

    fn process_get_history_min_height(&self) -> Result<Height> {
        Ok(self
            .state
            .store
            .min_finalized_height()?
            .map(Height::new)
            .unwrap_or_default())
    }

    async fn process_restream_proposal(
        &mut self,
        height: Height,
        round: Round,
        valid_round: Round,
        value_id: ValueId,
    ) -> Result<()> {
        let proposal_round = if valid_round == Round::Nil {
            round
        } else {
            valid_round
        };
        if let Some(p) =
            self.state
                .store
                .get_undecided_proposal(height, proposal_round, &value_id)?
        {
            let locally = LocallyProposedValue {
                height,
                round,
                value: p.value,
            };
            for stream_message in self.state.stream_proposal(locally, valid_round) {
                self.channels
                    .network
                    .send(NetworkMsg::PublishProposalPart(stream_message))
                    .await?;
            }
        }
        Ok(())
    }

    // ----------------------------- helpers ----------------------------

    /// Re-assemble + validate a complete [`ProposalParts`] stream
    /// against the application's [`Externalities::validate_block_above`].
    async fn assemble_and_validate(
        &self,
        parts: &ProposalParts,
    ) -> Result<ProposedValue<MalachiteCtx>> {
        let proposed = State::<P>::assemble_value_from_parts(parts.clone())?;
        let block = Block::<P>::decode(&mut &proposed.value.block_bytes[..])
            .map_err(|e| anyhow!("decoding Block from value bytes: {e}"))?;
        if block.height != proposed.height.as_u64() {
            return Err(anyhow!(
                "block.height ({}) does not match proposed height ({})",
                block.height,
                proposed.height
            ));
        }
        let local_parent = if proposed.height.as_u64() <= 1 {
            H256::zero()
        } else {
            self.state
                .store
                .finalized_block_at(proposed.height.as_u64() - 1)?
                .ok_or_else(|| {
                    anyhow!(
                        "no finalized block at height {} — Malachite invariant violated",
                        proposed.height.as_u64() - 1,
                    )
                })?
        };
        if block.parent_hash != local_parent {
            return Err(anyhow!(
                "parent_hash mismatch at height {}: block claims {:?}, local view {:?}",
                proposed.height,
                block.parent_hash,
                local_parent
            ));
        }
        // Parent + height already validated above. The application
        // only sees the parent hash + payload — payload-level checks
        // live there.
        let valid = self
            .externalities
            .validate_block_above(block.parent_hash, block.payload)
            .await
            .context("Externalities::validate_block_above")?;
        if !valid {
            return Err(anyhow!(
                "application rejected proposal at height {}",
                proposed.height
            ));
        }
        Ok(proposed)
    }

    /// Insert the freshly-finalized block into [`BlockEntry`] and run
    /// the strict-ordering save / finalize cascades against the
    /// application. Emits [`MalachiteEvent::BlockFinalized`] after
    /// every successful `mark_block_as_finalized` call (one event per
    /// block in chronological order, including any ancestors that
    /// became finalizable on this cascade).
    async fn ingest_finalized(&self, cert: EngineCert, block_bytes: Vec<u8>) -> Result<()> {
        let block = Block::<P>::decode(&mut &block_bytes[..])
            .map_err(|e| anyhow!("decoding Block at finalize: {e}"))?;
        let block_hash = block.hash();
        let height = cert.height.as_u64();

        let app_cert = CommitCertificate {
            height,
            block_hash,
            signatures: cert
                .commit_signatures
                .iter()
                .map(|sig| sig.signature.inner().to_bytes().to_vec())
                .collect(),
        };

        self.state.store.insert_block(BlockEntry::<P> {
            block_hash,
            parent_hash: block.parent_hash,
            height,
            payload: block.payload,
            reserved: block.reserved,
            saved: false,
            finalized: false,
            cert: Some(app_cert),
        })?;

        self.state
            .store
            .cascade_save(vec![block_hash], |hash, blk| {
                let ext = Arc::clone(&self.externalities);
                let tx = self.event_tx.clone();
                async move {
                    ext.save_block(hash, blk).await?;
                    let _ = tx.send(Ok(MalachiteEvent::BlockProposal { block_hash: hash }));
                    Ok(())
                }
            })
            .await?;
        self.state
            .store
            .cascade_finalize(vec![block_hash], |hash, cert| {
                let ext = Arc::clone(&self.externalities);
                let tx = self.event_tx.clone();
                async move {
                    ext.mark_block_as_finalized(hash, cert).await?;
                    let _ = tx.send(Ok(MalachiteEvent::BlockFinalized { block_hash: hash }));
                    Ok(())
                }
            })
            .await?;
        Ok(())
    }
}

fn compute_value_id_from_parts(parts: &ProposalParts) -> ValueId {
    use sha3::{Digest as _, Keccak256};
    let mut h = Keccak256::new();
    h.update(b"mala-svc/value-id-from-parts:v1:");
    h.update(parts.height.as_u64().to_be_bytes());
    h.update(parts.round.as_i64().to_be_bytes());
    h.update(parts.proposer.0.0);
    if let Some(bytes) = parts.data_block_bytes() {
        h.update(bytes);
    }
    ValueId(h.finalize().into())
}
