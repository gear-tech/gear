// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Channel-app event loop. Translates malachite [`AppMsg`]s into:
//!
//! - calls into [`crate::Externalities`] (build / validate /
//!   process proposal / finalize),
//! - storage operations against the [`crate::store::Store`].
//!
//! `process_mb_proposal` is invoked as soon as a proposal is
//! assembled and validated (in [`AppMsgHandler::process_get_value`]
//! for the local proposer, in
//! [`AppMsgHandler::process_received_proposal_part`] for peer
//! proposals, and in [`AppMsgHandler::process_synced_value`] for
//! sync-path values). `process_mb_finalized` runs only when the
//! engine commits a height, and assumes its block has already been
//! processed. The strict ordering documented on
//! [`crate::Externalities`] is enforced by
//! [`Store::cascade_save`] / [`Store::cascade_finalize`]; fatal
//! callback errors are surfaced through
//! [`crate::MalachiteService`]'s error stream.
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
    types::{Address, Block, CommitCertificate, H256},
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
use malachitebft_core_types::Height as _;
use parity_scale_codec::{Decode, Encode};
use std::{ops::RangeInclusive, sync::Arc};
use tracing::{error, info, warn};

/// Max allowed distance into the future for pending proposal parts.
const FUTURE_HEIGHT_WINDOW: u64 = 4;

type EngineCert = malachitebft_core_types::CommitCertificate<MalachiteCtx>;

/// Run the channel-app event loop. Terminates when the consensus
/// channel closes (engine shut down). Non-terminating errors raised
/// by individual `process_*` steps are forwarded to `errors_tx`;
/// terminating errors propagate out of this future.
pub async fn run<P, EXT>(
    state: State<P>,
    channels: Channels<MalachiteCtx>,
    externalities: Arc<EXT>,
) -> Result<()>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
    AppMsgHandler {
        state,
        channels,
        externalities,
    }
    .run()
    .await
}

enum FinalizationError {
    Fatal(anyhow::Error),
    NonFatal(anyhow::Error),
}

/// Owns the channel-app event-loop state and dispatches each
/// [`AppMsg`] variant to its matching `process_*` method. The dispatch
/// holds the engine reply channel — `process_*` only produces the
/// value (or fails) and never touches the reply itself.
struct AppMsgHandler<P: BlockPayload, EXT: Externalities<P>> {
    state: State<P>,
    channels: Channels<MalachiteCtx>,
    externalities: Arc<EXT>,
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
                    Err(FinalizationError::NonFatal(e)) => {
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
                    Err(FinalizationError::Fatal(e)) => {
                        return Err(anyhow!("Fatal error during finalization: {e:?}"));
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

        // Promote buffered parts into undecided proposals. The
        // record_assembled_block call is load-bearing: without it
        // ingest_finalized's strict-ordering debug_assert fires.
        let pending = self.state.store.get_pending_proposal_parts(height, round)?;
        for parts in pending {
            let promote = async {
                let proposed = self.assemble_and_validate(&parts).await?;
                self.state.store.store_undecided_proposal(&proposed)?;
                let block = Block::<P>::decode(&mut &proposed.value.block_bytes[..])
                    .map_err(|e| anyhow!("decoding Block from pending proposal: {e}"))?;
                self.record_assembled_block(block).await
            };
            if let Err(e) = promote.await {
                error!(?e, "rejecting invalid pending proposal");
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
        let locally = self
            .state
            .build_locally_proposed_value(height, round, block_bytes)?;
        // Hook process_mb_proposal at proposal-assembly time on the
        // proposer side. cascade_save guarantees ancestor-first
        // ordering against the application.
        self.record_assembled_block(block).await?;
        Ok(locally)
    }

    fn process_extend_vote(&self) -> Option<Bytes> {
        None
    }

    fn process_verify_vote_extension(&self) -> Result<(), VoteExtensionError> {
        Ok(())
    }

    // TODO: #5475 add per-peer token-bucket rate limit before `ingest_proposal_part`
    //       (CPU/bandwidth bound; complements the memory bound from #5473).
    // TODO: #5480 gate `from` against a validator-peer-id allowlist so random
    //       gossip-mesh peers can't reach this code path at all.
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
        if parts.height > self.state.current_height.increment_by(FUTURE_HEIGHT_WINDOW) {
            info!(parts.height = %parts.height, "Dropping proposal too far in the future");
            return Ok(None);
        }

        if parts.height > self.state.current_height {
            // TODO: #5476 verify `ProposalFin` signature against the expected
            //       proposer's pubkey BEFORE persisting — otherwise a Byzantine
            //       peer can use this DB write as a write-amplified DoS sink.
            // Buffer until the engine catches up to that height.
            let value_id = compute_value_id_from_parts(&parts);
            self.state
                .store
                .store_pending_proposal_parts(&parts, &value_id)?;
            Ok(None)
        } else {
            let proposed = self.assemble_and_validate(&parts).await?;
            self.state.store.store_undecided_proposal(&proposed)?;
            let block = Block::<P>::decode(&mut &proposed.value.block_bytes[..])
                .map_err(|e| anyhow!("decoding Block from received proposal: {e}"))?;
            self.record_assembled_block(block).await?;
            Ok(Some(proposed))
        }
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

    async fn process_finalized(
        &mut self,
        certificate: EngineCert,
    ) -> Result<(), FinalizationError> {
        let (block_bytes, _cert) = self
            .state
            .commit(certificate.clone())
            .map_err(FinalizationError::NonFatal)?;
        self.ingest_finalized(certificate, block_bytes)
            .await
            .context("ingest finalized")
            .map_err(FinalizationError::Fatal)
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
            // Sync-path: the engine delivers raw decided values in
            // ancestor-first order, so by the time
            // `record_assembled_block` runs the parent has already
            // been processed (cascade_save would be a no-op on a
            // missing ancestor anyway — see `Store::save_chain`).
            let block = Block::<P>::decode(&mut &proposed.value.block_bytes[..])
                .map_err(|e| anyhow!("decoding Block from synced value: {e}"))?;
            self.record_assembled_block(block).await?;
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

    /// Drive the strict-ordering save cascade against the application
    /// for a freshly-assembled block. Inserts a `saved=false,
    /// finalized=false, cert=None` [`BlockEntry`] and runs
    /// [`Store::cascade_save`] from this hash — the cascade flushes
    /// every ancestor that is now connected in chronological
    /// (parent-first) order. If an ancestor is still missing the
    /// cascade is a no-op; it will pick up when the gap closes via a
    /// later assembled block at the missing height.
    ///
    /// Called from [`Self::process_get_value`] (we are proposer),
    /// [`Self::process_received_proposal_part`] (peer proposal), and
    /// [`Self::process_synced_value`] (sync path). Multiple callers
    /// can race for the same `block_hash` — `Store::insert_block`
    /// dedup is idempotent and `cascade_save` skips already-saved
    /// entries, so the application's `process_mb_proposal` runs at
    /// most once per `block_hash`.
    async fn record_assembled_block(&self, block: Block<P>) -> Result<()> {
        let block_hash = block.hash();
        self.state.store.insert_block(BlockEntry::<P> {
            block_hash,
            parent_hash: block.parent_hash,
            height: block.height,
            payload: block.payload,
            reserved: block.reserved,
            saved: false,
            finalized: false,
            cert: None,
        })?;
        self.state
            .store
            .cascade_save(vec![block_hash], |hash, blk| {
                let ext = Arc::clone(&self.externalities);
                async move { ext.process_mb_proposal(hash, blk).await }
            })
            .await
    }

    /// Attach the engine's quorum certificate to the
    /// already-processed [`BlockEntry`] and run the finalize cascade.
    ///
    /// Contract: the block (and every ancestor) must have already been
    /// processed by [`Self::record_assembled_block`] earlier — the
    /// strict-ordering guarantee documented on
    /// [`crate::Externalities::process_mb_proposal`]. A debug-build
    /// assertion catches a violation; in release builds
    /// [`Store::cascade_finalize`] silently no-ops on an unsaved
    /// ancestor (the `finalize_chain` walk returns `None`), and the
    /// `errors_tx` channel surfaces the contract breach upstream.
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

        // Idempotent: when the entry already exists (the common case
        // because record_assembled_block ran first) `insert_block`
        // promotes the cert into it. The remaining fields are picked
        // for the rare "we never saw the proposal" recovery path —
        // the debug_assert below catches it.
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

        // Contract check: every block reachable from `block_hash`
        // through the unfinalized parent chain must have already
        // been processed via record_assembled_block. The
        // process_mb_proposal cascade for ancestors must have
        // completed before we reach finalization, otherwise our
        // strict-ordering invariant is broken.
        debug_assert!(
            self.all_ancestors_saved(block_hash)?,
            "ingest_finalized: block {block_hash} (or an unfinalized ancestor) is not saved — \
             record_assembled_block must have run before finalize",
        );

        self.state
            .store
            .cascade_finalize(vec![block_hash], |hash, cert| {
                let ext = Arc::clone(&self.externalities);
                async move { ext.process_mb_finalized(hash, cert).await }
            })
            .await?;
        Ok(())
    }

    /// True iff every block on the unfinalized parent chain rooted at
    /// `leaf_hash` is `saved=true`. Used as a debug-build invariant
    /// check inside [`Self::ingest_finalized`] — see its contract.
    fn all_ancestors_saved(&self, leaf_hash: H256) -> Result<bool> {
        let mut current = leaf_hash;
        loop {
            let Some(entry) = self.state.store.get_block(current)? else {
                return Ok(false);
            };
            if entry.finalized {
                return Ok(true);
            }
            if !entry.saved {
                return Ok(false);
            }
            if entry.parent_hash == H256::zero() {
                return Ok(true);
            }
            current = entry.parent_hash;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::{ProposalData, ProposalInit, Validator, ValidatorSet, Value},
        signing::{MalachiteSigner, libp2p_peer_id, private_key_from_bytes},
        state::SharedValidatorSet,
        store::Store,
    };
    use async_trait::async_trait;
    use malachitebft_app_channel::{
        ConsensusRequest, NetworkRequest,
        app::{events::TxEvent, streaming::StreamId},
    };
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    #[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
    struct TestPayload;

    struct NoopExt;

    #[async_trait]
    impl Externalities<TestPayload> for NoopExt {
        async fn process_mb_proposal(&self, _: H256, _: Block<TestPayload>) -> Result<()> {
            Ok(())
        }
        async fn process_mb_finalized(&self, _: H256, _: CommitCertificate) -> Result<()> {
            Ok(())
        }
        async fn build_block_above(&self, _: H256) -> Result<TestPayload> {
            Ok(TestPayload)
        }
        async fn validate_block_above(&self, _: H256, _: TestPayload) -> Result<bool> {
            Ok(true)
        }
    }

    fn test_signer(byte: u8) -> MalachiteSigner {
        let mut k = [0u8; 32];
        k[31] = byte;
        MalachiteSigner::new(private_key_from_bytes(&k).unwrap())
    }

    fn test_peer(byte: u8) -> PeerId {
        let mut bytes = [0u8; 32];
        bytes[31] = byte;
        let lp = libp2p_peer_id(&bytes);
        PeerId::from_bytes(&lp.to_bytes()).expect("valid multihash")
    }

    /// Init + Data + Fin for a fully-formed stream at `height`. The
    /// proposer field is filled in so [`ProposalParts`] assembles
    /// without complaint.
    fn complete_stream(
        proposer: Address,
        height: u64,
        payload: &[u8],
    ) -> Vec<StreamMessage<ProposalPart>> {
        let stream_id = StreamId::new(height.to_be_bytes().to_vec().into());
        vec![
            StreamMessage::new(
                stream_id.clone(),
                0,
                StreamContent::Data(ProposalPart::Init(ProposalInit::new(
                    Height::new(height),
                    Round::new(0),
                    Round::Nil,
                    proposer,
                ))),
            ),
            StreamMessage::new(
                stream_id.clone(),
                1,
                StreamContent::Data(ProposalPart::Data(ProposalData::new(payload.to_vec()))),
            ),
            StreamMessage::new(stream_id, 2, StreamContent::Fin),
        ]
    }

    /// Build an [`AppMsgHandler`] with the given `current_height` and
    /// a fresh on-disk store. Channels are dummy senders/receivers
    /// — `process_received_proposal_part` never touches them on the
    /// future-height paths under test.
    fn make_handler(
        current_height: u64,
    ) -> (AppMsgHandler<TestPayload, NoopExt>, TempDir, Address) {
        let dir = TempDir::new().unwrap();
        let store = Store::<TestPayload>::open(dir.path()).unwrap();
        let signer = test_signer(1);
        let address = Address::from_public_key(&signer.public_key());
        let validator_set = SharedValidatorSet::new(ValidatorSet::new(vec![Validator::new(
            signer.public_key(),
            1,
        )]));
        let mut state = State::<TestPayload>::new(
            signer,
            validator_set,
            address,
            store,
            Duration::from_secs(1),
        )
        .unwrap();
        state.current_height = Height::new(current_height);

        let (_consensus_tx, consensus_rx) = mpsc::channel::<AppMsg<MalachiteCtx>>(1);
        let (network_tx, _network_rx) = mpsc::channel::<NetworkMsg<MalachiteCtx>>(1);
        let (requests_tx, _requests_rx) = mpsc::channel::<ConsensusRequest<MalachiteCtx>>(1);
        let (net_requests_tx, _net_requests_rx) = mpsc::channel::<NetworkRequest>(1);
        let channels = Channels {
            consensus: consensus_rx,
            network: network_tx,
            events: TxEvent::new(),
            requests: requests_tx,
            net_requests: net_requests_tx,
        };

        let handler = AppMsgHandler {
            state,
            channels,
            externalities: Arc::new(NoopExt),
        };
        (handler, dir, address)
    }

    /// Replays a (peer, complete-stream) sequence through
    /// `process_received_proposal_part`, returning the final reply
    /// value (`Some(proposed)` on the same-height happy path, `None`
    /// when the parts were dropped or buffered).
    async fn run_stream(
        handler: &mut AppMsgHandler<TestPayload, NoopExt>,
        peer: PeerId,
        stream: Vec<StreamMessage<ProposalPart>>,
    ) -> Option<ProposedValue<MalachiteCtx>> {
        let mut last = None;
        for msg in stream {
            last = handler
                .process_received_proposal_part(peer, msg)
                .await
                .expect("process_received_proposal_part should not error");
        }
        last
    }

    /// A peer pumping completed proposals at heights beyond
    /// `current_height + FUTURE_HEIGHT_WINDOW` MUST be rejected at
    /// ingest time — otherwise an attacker can grow
    /// `store.pending_proposal_parts` indefinitely (every junk
    /// `value_id` is a fresh key, and `prune_engine_state` only
    /// sweeps `height ≤ current_height`).
    #[tokio::test]
    async fn far_future_proposal_parts_are_rejected_not_buffered() {
        let current = 10u64;
        let (mut handler, _dir, addr) = make_handler(current);
        let peer = test_peer(2);

        let far = current + FUTURE_HEIGHT_WINDOW + 1;
        let value = run_stream(&mut handler, peer, complete_stream(addr, far, b"junk")).await;
        assert!(value.is_none());

        let pending = handler
            .state
            .store
            .get_pending_proposal_parts(Height::new(far), Round::new(0))
            .unwrap();
        assert!(
            pending.is_empty(),
            "parts at height > current + FUTURE_HEIGHT_WINDOW must be dropped, \
             found {} pending entries at height {far}",
            pending.len(),
        );
    }

    /// Boundary case: `current + FUTURE_HEIGHT_WINDOW` is inside the
    /// allowed window (`>` not `>=`) and MUST be buffered. This pins
    /// the boundary so the inequality doesn't silently regress to
    /// off-by-one.
    #[tokio::test]
    async fn near_future_proposal_parts_within_window_are_buffered() {
        let current = 10u64;
        let (mut handler, _dir, addr) = make_handler(current);
        let peer = test_peer(2);

        let near = current + FUTURE_HEIGHT_WINDOW;
        let value = run_stream(&mut handler, peer, complete_stream(addr, near, b"hello")).await;
        assert!(value.is_none());

        let pending = handler
            .state
            .store
            .get_pending_proposal_parts(Height::new(near), Round::new(0))
            .unwrap();
        assert_eq!(
            pending.len(),
            1,
            "parts at height current + FUTURE_HEIGHT_WINDOW (boundary inside window) must be buffered",
        );
    }

    /// `Externalities` impl whose finalize-side callback always
    /// fails, so we can drive [`AppMsgHandler::process_finalized`]
    /// down the fatal path. `process_mb_proposal` must succeed so
    /// the prerequisite save cascade leaves the block in
    /// `saved=true` state — only then does `ingest_finalized`
    /// reach the finalize callback.
    struct FailingFinalizeExt;

    #[async_trait]
    impl Externalities<TestPayload> for FailingFinalizeExt {
        async fn process_mb_proposal(&self, _: H256, _: Block<TestPayload>) -> Result<()> {
            Ok(())
        }
        async fn process_mb_finalized(&self, _: H256, _: CommitCertificate) -> Result<()> {
            Err(anyhow!("application: finalize-side store write failed"))
        }
        async fn build_block_above(&self, _: H256) -> Result<TestPayload> {
            Ok(TestPayload)
        }
        async fn validate_block_above(&self, _: H256, _: TestPayload) -> Result<bool> {
            Ok(true)
        }
    }

    /// Same shape as [`make_handler`] but with a caller-supplied
    /// externalities impl. Used by the [`FinalizationError`] tests
    /// below that need to inject a failing callback.
    fn make_handler_with<EXT: Externalities<TestPayload>>(
        current_height: u64,
        ext: EXT,
    ) -> (AppMsgHandler<TestPayload, EXT>, TempDir, Address) {
        let dir = TempDir::new().unwrap();
        let store = Store::<TestPayload>::open(dir.path()).unwrap();
        let signer = test_signer(1);
        let address = Address::from_public_key(&signer.public_key());
        let validator_set = SharedValidatorSet::new(ValidatorSet::new(vec![Validator::new(
            signer.public_key(),
            1,
        )]));
        let mut state = State::<TestPayload>::new(
            signer,
            validator_set,
            address,
            store,
            Duration::from_secs(1),
        )
        .unwrap();
        state.current_height = Height::new(current_height);

        let (_consensus_tx, consensus_rx) = mpsc::channel::<AppMsg<MalachiteCtx>>(1);
        let (network_tx, _network_rx) = mpsc::channel::<NetworkMsg<MalachiteCtx>>(1);
        let (requests_tx, _requests_rx) = mpsc::channel::<ConsensusRequest<MalachiteCtx>>(1);
        let (net_requests_tx, _net_requests_rx) = mpsc::channel::<NetworkRequest>(1);
        let channels = Channels {
            consensus: consensus_rx,
            network: network_tx,
            events: TxEvent::new(),
            requests: requests_tx,
            net_requests: net_requests_tx,
        };

        let handler = AppMsgHandler {
            state,
            channels,
            externalities: Arc::new(ext),
        };
        (handler, dir, address)
    }

    /// Regression: an error returned by
    /// [`Externalities::process_mb_finalized`] MUST surface as
    /// [`FinalizationError::Fatal`] from
    /// [`AppMsgHandler::process_finalized`], so the dispatcher tears
    /// the app task down (`run` returns `Err`) instead of silently
    /// advancing the engine past a height the application never
    /// observed as finalized.
    ///
    /// The contract: a `NonFatal` mapping here would let the engine
    /// continue while the application is missing one or more
    /// `process_mb_finalized` calls — the strict-ordering invariant
    /// on [`Externalities`] would then be impossible to recover.
    #[tokio::test]
    async fn process_mb_finalized_error_propagates_as_fatal() {
        use malachitebft_core_types::Value as _;

        let height = 1u64;
        let (mut handler, _dir, _address) = make_handler_with(height, FailingFinalizeExt);

        // Locally-build a value so `state.commit` can resolve the cert
        // below. The genesis parent (`H256::zero()`) means the
        // ancestor walk inside `ingest_finalized` only inspects this
        // single block.
        let block = Block::<TestPayload>::new(H256::zero(), height, TestPayload);
        let block_bytes = block.encode();
        let proposed = handler
            .state
            .build_locally_proposed_value(Height::new(height), Round::new(0), block_bytes)
            .expect("build_locally_proposed_value");
        let value_id = proposed.value.id();

        // Run the proposal-assembly hook so the BlockEntry exists with
        // `saved=true` — the precondition for `ingest_finalized` to
        // reach the failing finalize callback (otherwise
        // `all_ancestors_saved` returns false and the debug-build
        // assertion fires before the cascade).
        handler
            .record_assembled_block(block)
            .await
            .expect("record_assembled_block must succeed under NoopFinalize proposal-side");

        // Forge a CommitCertificate carrying the matching `value_id`.
        // Signature bytes are irrelevant — `ingest_finalized` just
        // mirrors them into a `Vec<Vec<u8>>`.
        let cert = malachitebft_core_types::CommitCertificate {
            height: Height::new(height),
            round: Round::new(0),
            value_id,
            commit_signatures: Vec::new(),
        };

        match handler.process_finalized(cert).await {
            Err(FinalizationError::Fatal(_)) => {
                // Expected: app::run propagates the error and the
                // service tears down rather than silently moving on.
            }
            Err(FinalizationError::NonFatal(e)) => {
                panic!("expected FinalizationError::Fatal — got NonFatal: {e:?}")
            }
            Ok(()) => panic!("expected FinalizationError::Fatal — got Ok(())"),
        }
    }

    /// Regression for the multi-validator `tests::multiple_validators`
    /// failure: a proposer's parts can land on a peer **before** that
    /// peer's engine emits `process_started_round` for the same
    /// height, which routes them through the
    /// `pending_proposal_parts` buffer instead of the live
    /// [`AppMsgHandler::process_received_proposal_part`] path. Earlier
    /// the promotion in [`AppMsgHandler::process_started_round`] only
    /// called `store_undecided_proposal` — it did NOT run
    /// `record_assembled_block`, so the [`BlockEntry`] stayed
    /// `saved=false`. The engine then decided the block and
    /// [`AppMsgHandler::ingest_finalized`]'s `all_ancestors_saved`
    /// debug-build assertion fired.
    ///
    /// This test drives that exact sequence end-to-end with a single
    /// validator handler and asserts that finalize completes cleanly.
    #[tokio::test]
    async fn buffered_future_proposal_is_saved_on_promotion() {
        use malachitebft_core_types::Value as _;

        let height = 1u64;
        let (mut handler, _dir, address) = make_handler(0);
        let peer = test_peer(2);

        // Build a real Block so `assemble_and_validate` can decode it
        // during promotion. Genesis parent keeps the
        // `finalized_block_at(height - 1)` lookup out of the picture.
        let block = Block::<TestPayload>::new(H256::zero(), height, TestPayload);
        let block_bytes = block.encode();
        let block_hash = block.hash();

        // 1. Engine sits at height 0 → height-1 parts go into the buffer.
        let stream = complete_stream(address, height, &block_bytes);
        let received = run_stream(&mut handler, peer, stream).await;
        assert!(received.is_none(), "future-height parts must be buffered");
        assert!(
            handler.state.store.get_block(block_hash).unwrap().is_none(),
            "buffered parts must NOT produce a BlockEntry yet",
        );

        // 2. Engine reaches the buffered height → promotion fires.
        handler
            .process_started_round(Height::new(height), Round::new(0), address, Role::Validator)
            .await
            .expect("process_started_round must succeed on buffered parts");

        // 3. With the fix in place, promotion ran `record_assembled_block`,
        //    so the BlockEntry now exists with `saved=true`. Without the
        //    fix, the entry would either be missing or `saved=false`.
        let entry = handler
            .state
            .store
            .get_block(block_hash)
            .unwrap()
            .expect("promoted block must be inserted as a BlockEntry");
        assert!(
            entry.saved,
            "promoted future-height proposal must be marked saved — \
             record_assembled_block was skipped on the promotion path",
        );

        // 4. Drive finalize through the same path the engine takes.
        //    The debug_assert inside `ingest_finalized` is the regression
        //    sentinel: without the fix it panics here.
        let value_id = Value::new(block_bytes).id();
        let cert = malachitebft_core_types::CommitCertificate {
            height: Height::new(height),
            round: Round::new(0),
            value_id,
            commit_signatures: Vec::new(),
        };
        match handler.process_finalized(cert).await {
            Ok(()) => {}
            Err(FinalizationError::Fatal(e)) => panic!("Fatal: {e:?}"),
            Err(FinalizationError::NonFatal(e)) => panic!("NonFatal: {e:?}"),
        }
    }
}
