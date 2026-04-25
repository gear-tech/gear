// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Malachite channel-app event loop for ethexe.
//!
//! Runs in a spawned tokio task; consumes `AppMsg`s from the engine,
//! mirrors each decision back out on an internal `mpsc` as
//! `MalachiteEvent::{BlockProposal, BlockFinalized}` for the outer
//! `MalachiteService` stream. A second mpsc carries the latest
//! observer-delivered Ethereum chain head into `State` so the
//! producer can anchor the next sequencer block to the right EB.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use ethexe_common::{SimpleBlockData, injected::SignedInjectedTransaction};
use gprimitives::H256;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use malachitebft_app_channel::app::engine::host::{HeightParams, Next};
use malachitebft_app_channel::app::streaming::StreamContent;
use malachitebft_app_channel::app::types::core::utils::height::HeightRangeExt;
use malachitebft_app_channel::app::types::core::{Height as _, Round, Validity, Value as _};
use malachitebft_app_channel::app::types::sync::RawDecidedValue;
use malachitebft_app_channel::app::types::ProposedValue;
use malachitebft_app_channel::{AppMsg, Channels, NetworkMsg};

use crate::context::{EthexeContext, Height};
use crate::state::{State, decode_value, encode_value};
use crate::{CommitCertificate, MalachiteEvent, Mempool};

/// Run the channel-app event loop. Terminates when either the consensus
/// channel is closed (engine shut down) or `event_tx` is dropped (outer
/// service shut down).
pub async fn run(
    mut state: State,
    mut channels: Channels<EthexeContext>,
    mempool: Arc<dyn Mempool>,
    gas_allowance: u64,
    mut chain_head_rx: mpsc::UnboundedReceiver<SimpleBlockData>,
    event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
) -> Result<()> {
    loop {
        tokio::select! {
            // Latest observer-delivered chain head — overwrites the
            // previous value; we never keep a history. Also fans out
            // to the mempool so it can GC expired entries and
            // seen-hashes.
            Some(head) = chain_head_rx.recv() => {
                state.set_latest_received_head(head);
                mempool.set_chain_head(head);
            }

            // Messages from the Malachite engine
            msg = channels.consensus.recv() => {
                let Some(msg) = msg else {
                    return Err(anyhow!("Consensus channel closed unexpectedly"));
                };
                handle_app_msg(
                    &mut state,
                    &mut channels,
                    &*mempool,
                    gas_allowance,
                    &mut chain_head_rx,
                    &event_tx,
                    msg,
                )
                .await?;
            }
        }
    }
}

async fn handle_app_msg(
    state: &mut State,
    channels: &mut Channels<EthexeContext>,
    mempool: &dyn Mempool,
    gas_allowance: u64,
    chain_head_rx: &mut mpsc::UnboundedReceiver<SimpleBlockData>,
    event_tx: &mpsc::UnboundedSender<Result<MalachiteEvent>>,
    msg: AppMsg<EthexeContext>,
) -> Result<()> {
    match msg {
        // --- ConsensusReady ---------------------------------------------
        AppMsg::ConsensusReady { reply, .. } => {
            let start_height = state
                .store
                .max_decided_value_height()
                .await
                .map(|height| height.increment())
                .unwrap_or_else(|| Height::INITIAL);

            info!(%start_height, "Consensus is ready");

            sleep(Duration::from_millis(200)).await;

            let params = HeightParams::new(
                state.get_validator_set(start_height),
                state.get_timeouts(start_height),
                None,
            );
            if reply.send((start_height, params)).is_err() {
                error!("Failed to send ConsensusReady reply");
            }
        }

        // --- StartedRound -----------------------------------------------
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

            // Promote any pending parts for this (height, round) to undecided
            let pending_parts = state.store.get_pending_proposal_parts(height, round).await?;
            for parts in &pending_parts {
                state.store.remove_pending_proposal_parts(parts.clone()).await?;
                match state.validate_proposal_parts(parts) {
                    Ok(()) => {
                        let value = State::assemble_value_from_parts(parts.clone())?;
                        state.store.store_undecided_proposal(value).await?;
                    }
                    Err(e) => {
                        error!(?e, "Rejecting invalid pending proposal");
                    }
                }
            }

            let proposals = state.store.get_undecided_proposals(height, round).await?;
            if reply_value.send(proposals).is_err() {
                error!("Failed to send undecided proposals");
            }
        }

        // --- GetValue (we are proposer) ---------------------------------
        AppMsg::GetValue {
            height,
            round,
            timeout: _,
            reply,
        } => {
            info!(%height, %round, "Consensus requesting value to propose");

            // Engine may re-ask GetValue for the same (height, round) —
            // for example after a re-broadcast or a restart. Reusing
            // the previously stored proposal avoids:
            //   1. wasted block-assembly work,
            //   2. non-determinism (mempool / quarantine head can have
            //      shifted since the first build),
            //   3. duplicate / divergent BlockProposal events flowing
            //      out to the executor.
            let proposal = match state.get_previously_built_value(height, round).await? {
                Some(p) => {
                    info!(value = %p.value.id(), "Re-using previously built value");
                    p
                }
                None => {
                    // Producer pacing decision tree
                    // (mb_meta.last_advanced_block on parent MB
                    // tells us where the previous MB chain pinned
                    // the executor's view of the Ethereum chain):
                    //
                    //   1. New advance available? — quarantine-passed
                    //      candidate is a *strict* descendant of
                    //      `last_advanced_block`. Propose immediately
                    //      with `AdvanceTillEthereumBlock(candidate)`
                    //      plus whatever injected txs we have.
                    //
                    //   2. Candidate exists but is **not** a
                    //      descendant of `last_advanced_block` — that
                    //      means a previously-pinned EB was reorged
                    //      off the canonical chain (extraordinarily
                    //      unlikely past the K-block quarantine).
                    //      Log error and refuse to advance this MB;
                    //      keep proposing if there are injected txs,
                    //      otherwise fall through to wait.
                    //
                    //   3. No new advance, but injected txs present —
                    //      propose immediately with txs only.
                    //
                    //   4. Nothing to propose — wait. Wake up on
                    //      either a new chain-head event (re-check
                    //      whether the quarantine candidate moved)
                    //      or a new tx in the mempool. No deadline:
                    //      ETH provides a fresh slot every ~12s, so
                    //      in practice the wait is bounded; on a
                    //      true ETH outage the round timer rotates
                    //      proposers but no MB is produced until
                    //      content arrives somewhere.
                    let (advance, injected) = wait_for_proposable_content(
                        state,
                        mempool,
                        chain_head_rx,
                        gas_allowance,
                    )
                    .await;

                    let mut transactions = Vec::with_capacity(injected.len() + 3);
                    if let Some(eth_block_hash) = advance {
                        transactions.push(crate::Transaction::AdvanceTillEthereumBlock {
                            eth_block_hash,
                        });
                    }
                    for tx in injected {
                        transactions.push(crate::Transaction::Injected(tx));
                    }
                    transactions.push(crate::Transaction::ProgressTasks {
                        limits: crate::ProgressTasksLimits::default(),
                    });
                    transactions.push(crate::Transaction::ProcessQueues {
                        limits: crate::ProcessQueuesLimits::default(),
                    });
                    // Self-contained block: parent links to the last
                    // MB this node finalized (zero for the genesis
                    // MB).
                    let block =
                        crate::SequencerBlock::new(state.latest_finalized_mb_hash, transactions);

                    // Only emit the BlockProposal once we've actually
                    // built a fresh proposal — the cached-reuse
                    // branch above doesn't create new content.
                    let _ = event_tx.send(Ok(crate::MalachiteEvent::BlockProposal {
                        height: height.as_u64(),
                        block: block.clone(),
                    }));

                    state.propose_value(height, round, block).await?
                }
            };

            if reply.send(proposal.clone()).is_err() {
                error!("Failed to send GetValue reply");
            }

            for stream_message in state.stream_proposal(proposal, Round::Nil) {
                channels
                    .network
                    .send(NetworkMsg::PublishProposalPart(stream_message))
                    .await?;
            }
        }

        // --- Vote extensions (unused — return defaults) -----------------
        AppMsg::ExtendVote { reply, .. } => {
            if reply.send(None).is_err() {
                error!("Failed to send ExtendVote reply");
            }
        }
        AppMsg::VerifyVoteExtension { reply, .. } => {
            if reply.send(Ok(())).is_err() {
                error!("Failed to send VerifyVoteExtension reply");
            }
        }

        // --- ReceivedProposalPart (we are not proposer) -----------------
        AppMsg::ReceivedProposalPart { from, part, reply } => {
            let part_type = match &part.content {
                StreamContent::Data(part) => part.get_type(),
                StreamContent::Fin => "end of stream",
            };
            info!(%from, %part.sequence, part.type = %part_type, "Received proposal part");

            let proposed_value = state.received_proposal_part(from, part).await?;

            // If the stream assembled into a complete value and validated
            // OK, announce it outward as well.
            if let Some(ref pv) = proposed_value {
                let _ = event_tx.send(Ok(crate::MalachiteEvent::BlockProposal {
                    height: pv.height.as_u64(),
                    block: pv.value.block.clone(),
                }));
            }

            if reply.send(proposed_value).is_err() {
                error!("Failed to send ReceivedProposalPart reply");
            }
        }

        // --- Decided (value chosen, awaiting finalization delay) --------
        AppMsg::Decided {
            certificate,
            extensions: _,
        } => {
            info!(
                height = %certificate.height,
                round = %certificate.round,
                value = %certificate.value_id,
                signatures = certificate.commit_signatures.len(),
                "Consensus decided — awaiting Finalized",
            );
            sleep(Duration::from_millis(200)).await;
        }

        // --- Finalized (committed by quorum) ----------------------------
        AppMsg::Finalized {
            certificate,
            extensions,
            evidence,
            reply,
        } => {
            info!(
                height = %certificate.height,
                round = %certificate.round,
                value = %certificate.value_id,
                signatures = certificate.commit_signatures.len(),
                evidence = ?evidence,
                "Consensus finalized height, committing",
            );

            // Compute the outgoing certificate payload BEFORE moving the
            // inner cert into `state.commit`.
            let out_cert = CommitCertificate {
                height: certificate.height.as_u64(),
                block_hash: value_id_to_h256(certificate.value_id),
                signatures: certificate
                    .commit_signatures
                    .iter()
                    .map(|sig| sig.signature.inner().to_bytes().to_vec())
                    .collect(),
            };

            match state.commit(certificate, extensions).await {
                Ok(committed_block) => {
                    // Drop the committed injected txs from the pool
                    // and record their hashes in the seen-map so
                    // duplicates can't reappear before the ref_block
                    // ages out.
                    let injected: Vec<_> = committed_block
                        .transactions
                        .iter()
                        .filter_map(|tx| match tx {
                            crate::Transaction::Injected(signed) => Some(signed.clone()),
                            _ => None,
                        })
                        .collect();
                    mempool.forget(&injected).await;

                    let _ = event_tx.send(Ok(MalachiteEvent::BlockFinalized {
                        cert: out_cert,
                        block: committed_block,
                    }));

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
                        error!("Finalized: Failed to send StartHeight reply");
                    }
                }
                Err(e) => {
                    let height = state.current_height;
                    error!(%e, %height, "Finalized: Commit failed — restarting height");
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
                        error!("Finalized: Failed to send RestartHeight reply");
                    }
                }
            }
        }

        // --- Sync path (lagging peer catching up) -----------------------
        AppMsg::ProcessSyncedValue {
            height,
            round,
            proposer,
            value_bytes,
            reply,
        } => {
            info!(%height, %round, "Processing synced value");
            if let Some(value) = decode_value(value_bytes) {
                let proposed = ProposedValue {
                    height,
                    round,
                    valid_round: Round::Nil,
                    proposer,
                    value,
                    validity: Validity::Valid,
                };
                state.store.store_undecided_proposal(proposed.clone()).await?;
                if reply.send(Some(proposed)).is_err() {
                    error!("Failed to send ProcessSyncedValue reply");
                }
            } else if reply.send(None).is_err() {
                error!("Failed to send ProcessSyncedValue reply");
            }
        }

        AppMsg::GetDecidedValues { range, reply } => {
            let mut values = Vec::new();
            for height in range.iter_heights() {
                if let Some(dv) = state.get_decided_value(height).await {
                    values.push(RawDecidedValue {
                        certificate: dv.certificate,
                        value_bytes: encode_value(&dv.value),
                    });
                }
            }
            if reply.send(values).is_err() {
                error!("Failed to send GetDecidedValues reply");
            }
        }

        AppMsg::GetHistoryMinHeight { reply } => {
            let min_height = state.get_earliest_height().await;
            if reply.send(min_height).is_err() {
                error!("Failed to send GetHistoryMinHeight reply");
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
            let proposal = state
                .store
                .get_undecided_proposal(height, proposal_round, value_id)
                .await?;
            if let Some(proposal) = proposal {
                let locally = malachitebft_app_channel::app::types::LocallyProposedValue {
                    height,
                    round,
                    value: proposal.value,
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

fn value_id_to_h256(id: crate::context::ValueId) -> gprimitives::H256 {
    id.0
}

/// Block the proposer until there is *something* worth putting into
/// a [`SequencerBlock`] — and return that something already
/// computed:
///
/// - `Some(eth_block_hash)` when a quarantine-passed EB is a strict
///   descendant of the parent MB's `last_advanced_block` (a genuine
///   advance);
/// - `None` for the advance otherwise (no new advance, or a
///   misbehaving candidate that we logged-and-skipped);
///
/// plus the freshly fetched mempool batch. The returned tuple is
/// either `(Some, _)` (advance available) or `(_, non-empty)` (txs
/// available); we never return `(None, [])` — in that case we keep
/// waiting on chain-head events and mempool inserts.
///
/// While waiting, every chain-head delivery is fanned out to `state`
/// and `mempool` (same as the outer `run` loop), so by the time we
/// return, the local view is fully up to date.
async fn wait_for_proposable_content(
    state: &mut State,
    mempool: &dyn Mempool,
    chain_head_rx: &mut mpsc::UnboundedReceiver<SimpleBlockData>,
    gas_allowance: u64,
) -> (Option<H256>, Vec<SignedInjectedTransaction>) {
    use ethexe_common::db::MbStorageRO;

    let parent_advanced = if state.latest_finalized_mb_hash.is_zero() {
        H256::zero()
    } else {
        state
            .db
            .mb_meta(state.latest_finalized_mb_hash)
            .last_advanced_block
    };

    loop {
        // Re-evaluate every iteration — chain head and mempool can
        // both shift while we're waiting.
        let advance = compute_advance_candidate(state, parent_advanced);
        let injected = match state.latest_received_head {
            Some(head) => mempool.fetch(head, gas_allowance).await,
            None => Vec::new(),
        };
        if advance.is_some() || !injected.is_empty() {
            return (advance, injected);
        }

        // Nothing to propose — wait for any signal.
        tokio::select! {
            biased;
            head = chain_head_rx.recv() => {
                let Some(head) = head else {
                    // Producer of chain heads dropped: no more
                    // input is coming. Best we can do is propose an
                    // empty block to keep liveness; bail out of the
                    // wait without an advance.
                    warn!("chain_head channel closed; producing without advance");
                    return (None, Vec::new());
                };
                state.set_latest_received_head(head);
                mempool.set_chain_head(head);
            }
            _ = mempool.wait_for_new_tx() => {
                // Loop and re-check via fetch — wait_for_new_tx is
                // best-effort, the new tx may or may not pass the
                // canonical-ancestor filter.
            }
        }
    }
}

/// Resolve the next `AdvanceTillEthereumBlock` candidate for the
/// producer, given the parent MB's `last_advanced_block`.
///
/// - Returns `Some(candidate)` when the local quarantine anchor is a
///   strict descendant of `parent_advanced` (genuinely newer EB).
/// - Returns `None` when there's no candidate, when it equals the
///   parent's anchor (already pinned), or when the descendant check
///   surfaces a structural issue (logs at `warn`/`error` accordingly
///   and refuses to advance — the produced MB will still carry
///   tasks/queues plus any mempool content).
fn compute_advance_candidate(state: &State, parent_advanced: H256) -> Option<H256> {
    let candidate = match state.quarantine_anchor() {
        Ok(Some(c)) => c,
        Ok(None) => return None,
        Err(e) => {
            warn!(error = %e, "quarantine anchor lookup failed; skipping advance");
            return None;
        }
    };
    if candidate == parent_advanced {
        return None;
    }
    match state.is_strict_descendant_of(candidate, parent_advanced) {
        Ok(true) => Some(candidate),
        Ok(false) => None,
        Err(e) => {
            error!(
                error = %e,
                candidate = %candidate,
                parent_advanced = %parent_advanced,
                "quarantine-passed EB is not a canonical descendant of \
                 parent's last_advanced_block — possible deep reorg of an \
                 already-advanced block. Skipping AdvanceTillEthereumBlock \
                 for this proposal."
            );
            None
        }
    }
}
