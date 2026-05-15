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

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use parity_scale_codec::{Decode, Encode};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use malachitebft_app_channel::{
    AppMsg, Channels, NetworkMsg,
    app::{
        engine::host::{HeightParams, Next},
        streaming::StreamContent,
        types::{
            ProposedValue,
            core::{Height as _HeightTrait, Round, Validity, utils::height::HeightRangeExt},
            sync::RawDecidedValue,
        },
    },
};

use crate::{
    codec::{decode_value, encode_value},
    context::{Height, MalachiteCtx},
    externalities::{BlockPayload, Externalities},
    state::State,
    store::BlockEntry,
    types::{Block, CommitCertificate, H256, MalachiteEvent},
};

/// Run the channel-app event loop. Terminates when the consensus
/// channel closes (engine shut down).
pub async fn run<P, EXT>(
    mut state: State<P>,
    mut channels: Channels<MalachiteCtx>,
    externalities: Arc<EXT>,
    event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
) -> Result<()>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
    loop {
        let Some(msg) = channels.consensus.recv().await else {
            return Err(anyhow!("consensus channel closed"));
        };
        handle_app_msg(&mut state, &mut channels, &externalities, &event_tx, msg).await?;
    }
}

async fn handle_app_msg<P, EXT>(
    state: &mut State<P>,
    channels: &mut Channels<MalachiteCtx>,
    externalities: &Arc<EXT>,
    event_tx: &mpsc::UnboundedSender<Result<MalachiteEvent>>,
    msg: AppMsg<MalachiteCtx>,
) -> Result<()>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
    match msg {
        // ConsensusReady
        AppMsg::ConsensusReady { reply, .. } => {
            // Start at the height after the highest finalized block we
            // already know about — so a restarted node picks up
            // exactly where it left off.
            let start_height = state
                .store
                .max_finalized_height()?
                .map(|h| Height::new(h).increment())
                .unwrap_or_else(|| Height::INITIAL);
            info!(%start_height, "Consensus ready");

            state.current_height = start_height;
            let params = HeightParams::new(
                state.get_validator_set(start_height),
                state.get_timeouts(start_height),
                None,
            );
            if reply.send((start_height, params)).is_err() {
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
            state.current_height = height;
            state.current_round = round;
            state.current_proposer = Some(proposer);

            // Promote any pending parts buffered for this (height,
            // round) into proper undecided proposals.
            //
            // TODO +_+_+ : two fragility issues here, flagged 2/3 in
            // the audit iter 2.
            //
            // (2) The `?` on `remove_pending_proposal_parts` and
            // `store_undecided_proposal` propagates any RocksDB error
            // out of `handle_app_msg`, killing the app task BEFORE the
            // `reply_value.send(proposals)` reply below. The engine
            // awaits that reply forever — the validator stalls at this
            // height until restart. Fix: log+continue on per-entry
            // errors and always send the reply.
            //
            // Writing a unit reproduce requires a full engine fake;
            // TODO captures the issue with the exact line refs.
            let pending = state.store.get_pending_proposal_parts(height, round)?;
            for parts in pending {
                match assemble_and_validate(state, externalities, &parts).await {
                    Ok(proposed) => {
                        state.store.store_undecided_proposal(&proposed)?;
                    }
                    Err(e) => {
                        error!(?e, "rejecting invalid pending proposal");
                    }
                }
            }

            let proposals = state.store.get_undecided_proposals(height, round)?;
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

            let proposal = match state.get_previously_built_value(height, round)? {
                Some(p) => {
                    info!("re-using previously built value");
                    p
                }
                None => {
                    // Compute parent_hash from our finalized
                    // height-1 record. `H256::zero()` for genesis.
                    let parent_hash = if height.as_u64() <= 1 {
                        H256::zero()
                    } else {
                        state
                            .store
                            .finalized_block_at(height.as_u64() - 1)?
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "no finalized block at height {} — Malachite invariant violated",
                                    height.as_u64() - 1,
                                )
                            })?
                    };
                    let build_fut = externalities.build_block_above(parent_hash);
                    let payload = match tokio::time::timeout(state.propose_timeout, build_fut).await
                    {
                        Ok(Ok(p)) => p,
                        Ok(Err(e)) => {
                            error!(?e, "Externalities::build_block_above failed");
                            return Ok(());
                        }
                        Err(_) => {
                            warn!(
                                propose_timeout = ?state.propose_timeout,
                                "Externalities::build_block_above timed out"
                            );
                            return Ok(());
                        }
                    };
                    let block = Block::<P>::new(parent_hash, height.as_u64(), payload);
                    let block_bytes = block.encode();
                    state.build_locally_proposed_value(height, round, block_bytes)?
                }
            };

            if reply.send(proposal.clone()).is_err() {
                error!("GetValue: failed to send proposal reply");
            }
            for stream_message in state.stream_proposal(proposal, Round::Nil) {
                channels
                    .network
                    .send(NetworkMsg::PublishProposalPart(stream_message))
                    .await?;
            }
        }

        // Vote extensions (unused — return defaults).
        AppMsg::ExtendVote { reply, .. } => {
            if reply.send(None).is_err() {
                error!("ExtendVote: failed to send reply");
            }
        }
        AppMsg::VerifyVoteExtension { reply, .. } => {
            if reply.send(Ok(())).is_err() {
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

            let proposed_value = match state.ingest_proposal_part(from, part) {
                Some(parts) => {
                    if parts.height < state.current_height {
                        info!(parts.height = %parts.height, "Dropping outdated proposal");
                        None
                    } else if parts.height > state.current_height {
                        // Buffer until the engine catches up to
                        // that height.
                        //
                        // TODO +_+_+ : a peer can pump completed
                        // `ProposalParts` at arbitrarily large heights
                        // (`value_id` is content-addressed → every junk
                        // payload is a fresh key) and the rows never get
                        // pruned: `prune_engine_state` only sweeps
                        // `height <= current_height`. RocksDB grows
                        // without bound. Pinned by the (ignored) test
                        // `store::tests::pending_proposal_parts_at_future_heights_persist_after_prune`.
                        // Fix needs: height-window cap (refuse to persist
                        // beyond `current_height + N`) plus per-peer rate
                        // limit. See also the related validator-peer-id
                        // allowlist mitigation in
                        // `streaming.rs::PartStreamsMap` TODO.
                        let value_id = compute_value_id_from_parts(&parts);
                        state
                            .store
                            .store_pending_proposal_parts(&parts, &value_id)?;
                        None
                    } else {
                        match assemble_and_validate(state, externalities, &parts).await {
                            Ok(proposed) => {
                                state.store.store_undecided_proposal(&proposed)?;
                                Some(proposed)
                            }
                            Err(e) => {
                                error!(?e, "rejecting invalid proposal");
                                None
                            }
                        }
                    }
                }
                None => None,
            };
            if reply.send(proposed_value).is_err() {
                error!("ReceivedProposalPart: failed to send reply");
            }
        }

        // Decided (info only — Finalized fires next).
        AppMsg::Decided { certificate, .. } => {
            info!(
                height = %certificate.height,
                round = %certificate.round,
                value = %certificate.value_id,
                signatures = certificate.commit_signatures.len(),
                "Decided"
            );
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

            match state.commit(certificate.clone()) {
                Ok((block_bytes, _cert)) => {
                    if let Err(e) = ingest_finalized::<P, EXT>(
                        state,
                        externalities,
                        certificate.clone(),
                        block_bytes,
                        event_tx,
                    )
                    .await
                    {
                        error!(?e, "ingest_finalized failed");
                        let _ = event_tx.send(Err(e));
                    }
                    if reply
                        .send(Next::Start(
                            state.current_height,
                            HeightParams::new(
                                state.get_validator_set(state.current_height),
                                state.get_timeouts(state.current_height),
                                None,
                            ),
                        ))
                        .is_err()
                    {
                        error!("Finalized: failed to send Next reply");
                    }
                }
                Err(e) => {
                    let height = state.current_height;
                    error!(?e, %height, "Finalized: commit failed — restarting height");
                    if reply
                        .send(Next::Restart(
                            height,
                            HeightParams::new(
                                state.get_validator_set(height),
                                state.get_timeouts(height),
                                None,
                            ),
                        ))
                        .is_err()
                    {
                        error!("Finalized: failed to send Restart reply");
                    }
                }
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
            let parsed = decode_value(value_bytes).map(|v| ProposedValue {
                height,
                round,
                valid_round: Round::Nil,
                proposer,
                value: v,
                validity: Validity::Valid,
            });
            if let Some(ref proposed) = parsed {
                state.store.store_undecided_proposal(proposed)?;
            }
            if reply.send(parsed).is_err() {
                error!("ProcessSyncedValue: failed to send reply");
            }
        }

        AppMsg::GetDecidedValues { range, reply } => {
            let mut values = Vec::new();
            for height in range.iter_heights() {
                if let Some(dv) = state.get_decided_value(height) {
                    values.push(RawDecidedValue {
                        certificate: dv.certificate,
                        value_bytes: encode_value(&dv.value),
                    });
                }
            }
            if reply.send(values).is_err() {
                error!("GetDecidedValues: failed to send reply");
            }
        }

        AppMsg::GetHistoryMinHeight { reply } => {
            let min = state
                .store
                .min_finalized_height()?
                .map(Height::new)
                .unwrap_or_default();
            if reply.send(min).is_err() {
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
            let proposal_round = if valid_round == Round::Nil {
                round
            } else {
                valid_round
            };
            if let Some(p) =
                state
                    .store
                    .get_undecided_proposal(height, proposal_round, &value_id)?
            {
                let locally = malachitebft_app_channel::app::types::LocallyProposedValue {
                    height,
                    round,
                    value: p.value,
                };
                for stream_message in state.stream_proposal(locally, valid_round) {
                    channels
                        .network
                        .send(NetworkMsg::PublishProposalPart(stream_message))
                        .await?;
                }
            }
        }
    }
    Ok(())
}

// ----------------------------- helpers ---------------------------------

/// Re-assemble + validate a complete [`crate::streaming::ProposalParts`]
/// stream against the application's
/// [`Externalities::validate_block_above`].
async fn assemble_and_validate<P, EXT>(
    state: &State<P>,
    externalities: &Arc<EXT>,
    parts: &crate::streaming::ProposalParts,
) -> Result<ProposedValue<MalachiteCtx>>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
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
        state
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
    // Parent + height already validated above. The application only
    // sees the parent hash + payload — payload-level checks live
    // there.
    let valid = externalities
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
/// application. Emits [`MalachiteEvent::BlockFinalized`] after every
/// successful `mark_block_as_finalized` call (one event per block in
/// chronological order, including any ancestors that became
/// finalizable on this cascade).
async fn ingest_finalized<P, EXT>(
    state: &State<P>,
    externalities: &Arc<EXT>,
    cert: malachitebft_core_types::CommitCertificate<MalachiteCtx>,
    block_bytes: Vec<u8>,
    event_tx: &mpsc::UnboundedSender<Result<MalachiteEvent>>,
) -> Result<()>
where
    P: BlockPayload,
    EXT: Externalities<P>,
{
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

    state.store.insert_block(BlockEntry::<P> {
        block_hash,
        parent_hash: block.parent_hash,
        height,
        payload: block.payload,
        reserved: block.reserved,
        saved: false,
        finalized: false,
        cert: Some(app_cert),
    })?;

    state
        .store
        .cascade_save(vec![block_hash], |hash, blk| {
            let ext = Arc::clone(externalities);
            let tx = event_tx.clone();
            async move {
                ext.save_block(hash, blk).await?;
                let _ = tx.send(Ok(MalachiteEvent::BlockProposal { block_hash: hash }));
                Ok(())
            }
        })
        .await?;
    state
        .store
        .cascade_finalize(vec![block_hash], |hash, cert| {
            let ext = Arc::clone(externalities);
            let tx = event_tx.clone();
            async move {
                ext.mark_block_as_finalized(hash, cert).await?;
                let _ = tx.send(Ok(MalachiteEvent::BlockFinalized { block_hash: hash }));
                Ok(())
            }
        })
        .await?;
    Ok(())
}

fn compute_value_id_from_parts(parts: &crate::streaming::ProposalParts) -> crate::context::ValueId {
    use sha3::{Digest as _, Keccak256};
    let mut h = Keccak256::new();
    h.update(b"mala-svc/value-id-from-parts:v1:");
    h.update(parts.height.as_u64().to_be_bytes());
    h.update(parts.round.as_i64().to_be_bytes());
    h.update(parts.proposer.0.0);
    if let Some(bytes) = parts.data_block_bytes() {
        h.update(bytes);
    }
    crate::context::ValueId(h.finalize().into())
}
