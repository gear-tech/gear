// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`ethexe_malachite_core::Externalities`] glue for ethexe.
//!
//! ethexe-malachite-core is application-agnostic — it owns the BFT engine, the
//! libp2p swarm, and the persistent consensus state. Everything
//! ethexe-specific (block contents, validation rules, DB schema)
//! lives behind this trait.
//!
//! ## Map of responsibilities
//! - [`EthexeExternalities::process_mb_proposal`] — once
//!   ethexe-malachite-core has assembled and validated a proposal
//!   (parent already processed), persist the MB to the ethexe
//!   `mb_*` keyspace, propagate `last_advanced_eb`, and fire
//!   [`MalachiteEvent::BlockProposal`]. Called for sibling
//!   proposals too (one per round that produced an assembled MB) —
//!   only the finalized one ever flows through `process_mb_finalized`.
//! - [`EthexeExternalities::process_mb_finalized`] — flush the
//!   committed injected txs out of the mempool, advance
//!   `globals.latest_finalized_mb_hash`, and fire
//!   [`MalachiteEvent::BlockFinalized`].
//! - [`EthexeExternalities::build_block_above`] — when this node is
//!   proposer, wait for proposable content (a new EB past quarantine
//!   or a non-empty mempool), then assemble a [`Transactions`].
//! - [`EthexeExternalities::validate_block_above`] — for an incoming
//!   peer proposal, run ethexe's quarantine + parent-link checks
//!   before voting.
//!
//! ## Storage layout
//!
//! All MB-keyed storage in the ethexe DB is keyed by the
//! `ethexe_malachite_core::Block` envelope hash (Blake2b over
//! `(parent_hash, height, payload_hash, reserved)`).
//! [`EthexeExternalities::process_mb_proposal`] writes a [`CompactMb`]
//! under that key (carrying parent + height + the Blake2b hash of
//! the [`Transactions`] payload) and CAS-stores the `Transactions`
//! blob; [`EthexeExternalities::process_mb_finalized`] reads both
//! back via the same key the consensus layer hands in.

use crate::{
    CommitCertificate, MalachiteEvent, Mempool, quarantine,
    tx_validity::{TxValidity, TxValidityChecker, eb_touched_programs},
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{
    MAX_TOUCHED_PROGRAMS_PER_MB, SimpleBlockData,
    db::{CompactMb, GlobalsStorageRO, GlobalsStorageRW, MbStorageRO, MbStorageRW},
    injected::{MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB, SignedInjectedTransaction},
    malachite::{ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions},
};
use ethexe_db::Database;
use ethexe_malachite_core::{Block, Externalities};
use gprimitives::H256;
use parity_scale_codec::Encode;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
};
use tokio::sync::{Notify, mpsc};
use tracing::{error, info, warn};

/// Inputs the externalities need to satisfy the [`ethexe_malachite_core::Externalities`]
/// contract. Constructed by [`crate::MalachiteService::new`] and
/// handed to the inner ethexe-malachite-core service inside an [`Arc`].
pub(crate) struct EthexeExternalities {
    pub(crate) db: Database,
    pub(crate) mempool: Arc<dyn Mempool>,
    /// Latest Ethereum chain head observed via the outer
    /// [`crate::MalachiteService::receive_new_chain_head`]. The
    /// producer reads this from inside [`Self::build_block_above`];
    /// validators read it from inside [`Self::validate_block_above`].
    /// Decoupled from `globals.latest_synced_eb` because the latter
    /// trails the event stream and would block proposals that the
    /// observer has already announced.
    pub(crate) chain_head: Arc<RwLock<Option<SimpleBlockData>>>,
    /// Wakes up [`Self::wait_for_proposable_content`] whenever a
    /// fresh chain head arrives. Combines with the mempool's
    /// [`Mempool::wait_for_new_tx`] notify into a single select.
    pub(crate) chain_head_notify: Arc<Notify>,
    /// Outbound event channel — drained by
    /// [`crate::MalachiteService::poll_next`]. We wrap each emit in
    /// [`Self::try_emit_or_queue`] so that events whose
    /// `last_advanced_eb` Eth-block isn't fully synced into the
    /// local DB are held back until the observer catches up.
    pub(crate) event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
    /// Buffer for [`MalachiteEvent`]s whose downstream
    /// `compute_mb` walk would step through Eth blocks the
    /// observer hasn't synced yet. Drained in FIFO order by
    /// [`Self::drain_pending_events`] (called from
    /// [`crate::MalachiteService::receive_new_chain_head`]) —
    /// preserves the strict ordering of save / finalize cascades.
    pub(crate) pending_events: Mutex<VecDeque<PendingEvent>>,
    pub(crate) gas_allowance: u64,
    pub(crate) canonical_quarantine: u8,
    /// See [`crate::MalachiteConfig::post_quarantine_delay`]. Producer-side
    /// hint only: deepens the anchor in [`Self::find_eb_candidate_for_advancing`]
    /// so lagging validators are likely to have synced the proposed EB by the
    /// time they see the MB. Validators do NOT apply this depth — they accept
    /// any advance at depth ≥ `canonical_quarantine`.
    pub(crate) post_quarantine_delay: u32,
}

/// One outbound [`MalachiteEvent`] that can't be released until its
/// `prerequisite` Eth block is fully synced into the local DB.
pub(crate) struct PendingEvent {
    pub event: MalachiteEvent,
    /// Eth-block hash whose `block_events` entry must be present
    /// before this event can fire — i.e. the MB's
    /// `last_advanced_eb`. `H256::zero()` skips the gate (genesis
    /// or an MB that never advanced past the pre-genesis sentinel).
    pub prerequisite: H256,
}

#[async_trait]
impl Externalities<Transactions> for EthexeExternalities {
    async fn process_mb_proposal(&self, mb_hash: H256, mb: Block<Transactions>) -> Result<()> {
        let parent = mb.parent_hash;
        let payload = mb.payload;

        // Propagate `last_advanced_eb` forward — the latest
        // `AdvanceTillEthereumBlock` in this MB wins; otherwise we
        // inherit the parent's value (zero if pre-genesis).
        let parent_advanced = if parent.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent).last_advanced_eb
        };
        let last_advanced = payload
            .iter()
            .rev()
            .find_map(|tx| match tx {
                Transaction::AdvanceTillEthereumBlock { block_hash } => Some(*block_hash),
                _ => None,
            })
            .unwrap_or(parent_advanced);

        // CAS-store transactions first so the contract — "if
        // CompactMb exists, transactions are reachable" — holds
        // unconditionally.
        let transactions_hash = self.db.set_transactions(payload.clone());
        self.db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent,
                height: mb.height,
                transactions_hash,
            },
        );
        self.db.mutate_mb_meta(mb_hash, |meta| {
            meta.last_advanced_eb = last_advanced;
        });

        self.try_emit_or_queue(
            MalachiteEvent::BlockProposal {
                height: mb.height,
                mb_hash,
            },
            last_advanced,
        );
        Ok(())
    }

    async fn process_mb_finalized(
        &self,
        mb_hash: H256,
        cert: ethexe_malachite_core::CommitCertificate,
    ) -> Result<()> {
        let compact = self.db.mb_compact_block(mb_hash).ok_or_else(|| {
            anyhow!(
                "process_mb_finalized: no CompactMb for {mb_hash} \
                 (process_mb_proposal must run first)"
            )
        })?;
        let payload = self
            .db
            .transactions(compact.transactions_hash)
            .ok_or_else(|| {
                anyhow!(
                    "mark_finalized: transactions blob {} missing for block {mb_hash}",
                    compact.transactions_hash
                )
            })?;

        // Flush the committed injected txs from the mempool and add
        // their hashes to the seen-set so a re-gossip can't slip them
        // back in before they age out.
        let injected: Vec<SignedInjectedTransaction> = payload
            .iter()
            .filter_map(|tx| match tx {
                Transaction::Injected(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        if !injected.is_empty() {
            self.mempool.forget(&injected).await;
        }

        // Advance the canonical pointer downstream consumers
        // (compute, batch commitment) walk to find the last
        // BFT-finalized MB.
        self.db
            .globals_mutate(|g| g.latest_finalized_mb_hash = mb_hash);

        let app_cert = CommitCertificate {
            height: cert.height,
            mb_hash,
            signatures: cert.signatures,
        };
        // Same prerequisite as the matching BlockProposal — by the
        // time `process_mb_finalized` runs, `process_mb_proposal` has
        // already populated `mb_meta(block_hash).last_advanced_eb`.
        let last_advanced = self.db.mb_meta(mb_hash).last_advanced_eb;
        self.try_emit_or_queue(
            MalachiteEvent::BlockFinalized {
                cert: app_cert,
                height: cert.height,
                mb_hash,
            },
            last_advanced,
        );
        Ok(())
    }

    async fn build_block_above(&self, parent_mb_hash: H256) -> Result<Transactions> {
        // `parent_hash` is the consensus envelope hash of the parent
        // (zero for genesis). Use it directly to seed the producer's
        // `last_advanced_eb` lookup.
        let parent_advanced = if parent_mb_hash.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_mb_hash).last_advanced_eb
        };

        let (advance, injected) = self.wait_for_proposable_content(parent_advanced).await;

        info!(
            %parent_mb_hash,
            %parent_advanced,
            advance = ?advance,
            injected_count = injected.len(),
            "build_block_above: proposable content resolved",
        );

        // (a) Per-tx validity. Each candidate tx from the mempool is
        // run through TxValidityChecker so we don't waste an MB
        // round-trip on a tx the participant would reject.
        let chain_head_snapshot = *self.chain_head.read().expect("chain_head poisoned");
        let valid: Vec<SignedInjectedTransaction> = match chain_head_snapshot {
            Some(head) => {
                let checker = TxValidityChecker::new_for_mb(self.db.clone(), head, parent_mb_hash)?;
                let mut accepted = Vec::with_capacity(injected.len());
                for tx in injected {
                    match checker.check_tx_validity(&tx)? {
                        TxValidity::Valid => accepted.push(tx),
                        reason => {
                            warn!(
                                tx_hash = %tx.data().to_hash(),
                                ?reason,
                                "build_block_above: dropping injected tx — fails TxValidity",
                            );
                        }
                    }
                }
                accepted
            }
            // No chain head yet — we can't run TxValidity (no anchor
            // for `is_reference_block_*`). Skip injected txs entirely
            // rather than emit unvalidated ones.
            None => {
                if !injected.is_empty() {
                    warn!(
                        injected_count = injected.len(),
                        "build_block_above: no chain head — dropping injected txs (unvalidated)",
                    );
                }
                Vec::new()
            }
        };

        // (b) Per-MB size + touched-programs caps. Adapted from
        // master's `select_for_announce`:
        //
        // - size cap: cumulative `tx.encoded_size()` (with signature)
        //   ≤ MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB; oversized tx is
        //   skipped, smaller subsequent txs still get a chance.
        // - touched-programs cap: starts with `eb_touched_programs`
        //   over the EB range this MB is about to advance through;
        //   a tx whose destination isn't already in the touched set
        //   is dropped once the set reaches MAX_TOUCHED_PROGRAMS_PER_MB.
        //
        // If `advance` is `None`, no EB events are processed by this
        // MB → the touched-set seed is empty.
        let mut touched = match advance {
            Some(advanced_eb) => eb_touched_programs(&self.db, parent_advanced, advanced_eb)?,
            None => std::collections::HashSet::new(),
        };
        let initial_touched_count = touched.len();
        if initial_touched_count > MAX_TOUCHED_PROGRAMS_PER_MB as usize {
            // Producer can't shrink this — the EB events themselves
            // already exceed the cap. Drop injected txs and let the
            // MB advance the EB anyway so the chain progresses.
            warn!(
                initial_touched_count,
                limit = MAX_TOUCHED_PROGRAMS_PER_MB,
                "build_block_above: EB events already exceed touched-programs cap; \
                 dropping all injected txs from this MB",
            );
        }

        let mut size_counter: usize = 0;
        let mut capped: Vec<SignedInjectedTransaction> = Vec::with_capacity(valid.len());
        for tx in valid {
            // Skip the whole loop body once initial touched > limit —
            // any injected tx would only push it further over.
            if initial_touched_count > MAX_TOUCHED_PROGRAMS_PER_MB as usize {
                break;
            }

            let tx_size = tx.encoded_size();
            if size_counter + tx_size > MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB {
                // Master's behaviour: skip oversized tx but keep
                // trying smaller subsequent txs.
                continue;
            }

            let destination = tx.data().destination;
            if !touched.contains(&destination)
                && touched.len() >= MAX_TOUCHED_PROGRAMS_PER_MB as usize
            {
                // Adding this destination would breach the cap; skip.
                continue;
            }

            touched.insert(destination);
            size_counter += tx_size;
            capped.push(tx);
        }

        // Producer pacing:
        //   1. AdvanceTillEthereumBlock first (if a fresh
        //      quarantine-passed EB exists),
        //   2. then injected user txs,
        //   3. finally the service-level ProgressTasks +
        //      ProcessQueues bookend.
        let mut transactions = Vec::with_capacity(capped.len() + 3);
        if let Some(block_hash) = advance {
            transactions.push(Transaction::AdvanceTillEthereumBlock { block_hash });
        }
        for tx in capped {
            transactions.push(Transaction::Injected(tx));
        }
        transactions.push(Transaction::ProgressTasks {
            limits: ProgressTasksLimits::default(),
        });
        transactions.push(Transaction::ProcessQueues {
            limits: ProcessQueuesLimits {
                gas_allowance: self.gas_allowance,
            },
        });
        Ok(Transactions::new(transactions))
    }

    async fn validate_block_above(&self, parent_hash: H256, payload: Transactions) -> Result<bool> {
        // (1) Shape + ordering. Every honest MB has exactly the form:
        //
        //   [AdvanceTillEthereumBlock]?  Injected*  ProgressTasks  ProcessQueues
        //
        // This single walk catches: missing bookend, extra bookend,
        // out-of-order tx, more than one Advance, and the
        // `gas_allowance` cap. Everything else (TxValidity per injected
        // tx, EB quarantine, touched-programs cap) runs below assuming
        // the shape is sound.
        let mut iter = payload.iter();
        let mut next = iter.next();

        let advance: Option<H256> =
            if let Some(Transaction::AdvanceTillEthereumBlock { block_hash }) = next {
                let h = *block_hash;
                next = iter.next();
                Some(h)
            } else {
                None
            };

        while let Some(Transaction::Injected(_)) = next {
            next = iter.next();
        }

        let Some(Transaction::ProgressTasks { limits: _ }) = next else {
            warn!(
                "validate: MB shape violation — expected `ProgressTasks` bookend, got {:?}",
                next.map(|t| t.tag())
            );
            return Ok(false);
        };
        // `ProgressTasksLimits` is empty today; when fields are added,
        // bound them here.

        let Some(Transaction::ProcessQueues { limits: pq_limits }) = iter.next() else {
            warn!("validate: MB shape violation — expected `ProcessQueues` bookend");
            return Ok(false);
        };

        if pq_limits.gas_allowance > crate::MalachiteConfig::DEFAULT_GAS_ALLOWANCE {
            warn!(
                allowance = pq_limits.gas_allowance,
                cap = crate::MalachiteConfig::DEFAULT_GAS_ALLOWANCE,
                "validate: ProcessQueues.gas_allowance exceeds protocol cap"
            );
            return Ok(false);
        }

        if iter.next().is_some() {
            warn!("validate: MB has extra transactions after the `ProcessQueues` bookend");
            return Ok(false);
        }

        // (2) Quarantine + parent-link — single synchronous check.
        //
        // Validators never wait for local sync here. The proposer's
        // `post_quarantine_delay` config knob deepens its anchor by
        // ≥ 1 Hoodi block on top of `canonical_quarantine`, so the
        // referenced EB is almost certainly already in every
        // validator's DB by the time the MB arrives. If a validator's
        // observer is still behind (rare), we vote nil immediately —
        // round-rotation lets the next proposer try again — instead of
        // blocking the consensus app task on a poll loop.
        //
        // TODO: #5477 extract a shared `check_eb_advance` helper so this
        //       validator path and `find_eb_candidate_for_advancing` on the
        //       producer side stay in lockstep through future refactors.
        // TODO: #5479 emit `malachite_validate_abstain_total{reason=...}` at
        //       each early-return below so operators can tune
        //       `post_quarantine_delay` from observability rather than logs.
        if let Some(advance) = advance {
            let parent_advanced = if parent_hash.is_zero() {
                H256::zero()
            } else {
                self.db.mb_meta(parent_hash).last_advanced_eb
            };
            let start_block_hash = self.db.globals().start_block_hash;

            let Some(chain_head) = *self.chain_head.read().expect("chain_head poisoned") else {
                warn!(
                    %advance,
                    "validate: no local chain_head yet — rejecting MB with advance",
                );
                return Ok(false);
            };

            if let Err(e) = quarantine::verify_passed(
                &self.db,
                chain_head,
                advance,
                self.canonical_quarantine,
                start_block_hash,
            ) {
                warn!(
                    error = %e,
                    %advance,
                    parent_advanced = %parent_advanced,
                    "validate: advance not yet covered by local view — rejecting",
                );
                return Ok(false);
            }

            match quarantine::is_strict_descendant_of(
                &self.db,
                advance,
                parent_advanced,
                start_block_hash,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    warn!(
                        %advance,
                        parent_advanced = %parent_advanced,
                        "validate: advance not strict descendant of parent.last_advanced_eb — rejecting",
                    );
                    return Ok(false);
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        %advance,
                        parent_advanced = %parent_advanced,
                        "validate: is_strict_descendant_of failed — rejecting",
                    );
                    return Ok(false);
                }
            }
        }

        // (3) Injected-tx validity — every `Transaction::Injected` in
        // the proposed MB must pass the same checker the producer
        // applied in `build_block_above`. Reject the MB on the first
        // non-`Valid` outcome so the participant doesn't sign an MB
        // whose `compute_mb` would diverge from the proposer's.
        let chain_head_snapshot = *self.chain_head.read().expect("chain_head poisoned");
        let Some(chain_head) = chain_head_snapshot else {
            // No local chain head yet. If the MB carries no injected
            // txs we can still accept it; otherwise we must abstain
            // since the checker has no anchor to walk from.
            let has_injected = payload
                .iter()
                .any(|tx| matches!(tx, Transaction::Injected(_)));
            if has_injected {
                warn!("validate: MB carries injected txs but no local chain head — abstaining");
                return Ok(false);
            }
            return Ok(true);
        };

        // `?` here only fires on DB-invariant violations along the MB
        // ancestor walk (missing `mb_compact_block` for a non-zero MB on
        // the chain, or missing `mb_program_states` on an MB marked
        // `computed`). The `parent_hash` comes from the Malachite engine,
        // not the proposer, so malicious tx data can't reach this path.
        // Propagating the error upward is the right call: it indicates
        // local DB corruption, not a peer-side issue.
        let checker = TxValidityChecker::new_for_mb(self.db.clone(), chain_head, parent_hash)?;
        for tx in payload.iter() {
            let Transaction::Injected(signed) = tx else {
                continue;
            };
            // `?` inside `check_tx_validity` only fires on local DB
            // inconsistency (a `latest_states` entry whose `state_hash`
            // is absent from CAS). Every malicious-tx-data path returns
            // `Ok(TxValidity::<reason>)` instead of `Err`, so this `?`
            // can't be triggered by what the proposer placed in the MB.
            match checker.check_tx_validity(signed)? {
                TxValidity::Valid => {}
                reason => {
                    warn!(
                        tx_hash = %signed.data().to_hash(),
                        ?reason,
                        "validate: injected tx fails TxValidity — rejecting MB",
                    );
                    return Ok(false);
                }
            }
        }

        // (4) Touched-programs cap (master's #6). Only enforced on
        // the validator side — the proposer in `build_block_above`
        // already shapes the MB to stay within the cap; this check
        // is the participant's guard against a malicious proposer.
        //
        // Per master: `limit = max(initial_touched.len(), MAX_*)` —
        // the proposer can't *avoid* programs already touched by EB
        // events, so those set the floor for the cap. We add every
        // `Transaction::Injected` destination on top of the EB-touched
        // seed and reject if the union exceeds `limit`.
        //
        // NOTE: there is no per-MB size cap on the validator side
        // (master parity). We rely on the Malachite engine's 1 MiB
        // hard cap on the encoded `Block` payload — anything larger
        // never reaches `validate_block_above` in the first place.
        let parent_advanced = if parent_hash.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_hash).last_advanced_eb
        };
        // `?` here only fires on local DB issues: missing
        // `mb_program_states` for `latest_computed_mb_hash`, missing
        // `block_header` on a canonical ancestor of `advance`, or
        // missing `block_events` for one of them. After the quarantine
        // gate above succeeded the observer has clearly synced
        // `advance` and its ancestors, so any failure here is a local
        // DB / sync race — not a proposer-controlled condition. Same
        // reasoning as the other two `?`s in this function.
        let mut touched = match advance {
            Some(advanced_eb) => eb_touched_programs(&self.db, parent_advanced, advanced_eb)?,
            None => std::collections::HashSet::new(),
        };
        let limit = touched.len().max(MAX_TOUCHED_PROGRAMS_PER_MB as usize);
        for tx in payload.iter() {
            if let Transaction::Injected(signed) = tx {
                touched.insert(signed.data().destination);
            }
        }
        if touched.len() > limit {
            warn!(
                touched = touched.len(),
                limit, "validate: MB touches too many programs — rejecting"
            );
            return Ok(false);
        }

        Ok(true)
    }
}

impl EthexeExternalities {
    /// True iff `prerequisite.is_zero()` (no prerequisite — genesis
    /// or pre-advance) or the prerequisite Eth block has been fully
    /// **prepared** locally.
    ///
    /// "Prepared" (vs. merely observed via `block_events`) is the
    /// stronger condition we need: `prepare_block`'s pipeline
    /// transitions through `WaitingForCodes` and only flips
    /// `block_meta.prepared = true` once every code referenced by
    /// the block (and its ancestors) has been loaded and validated.
    /// Releasing the BlockProposal event on merely-observed (but not
    /// yet prepared) EBs would let downstream `compute_mb` race the
    /// code-validation pipeline and fail with `MissingCode` when an
    /// MB's advance chain contains a `ProgramCreated` event for a
    /// not-yet-validated code.
    fn prerequisite_satisfied(&self, prerequisite: H256) -> bool {
        use ethexe_common::db::BlockMetaStorageRO;
        prerequisite.is_zero() || self.db.block_meta(prerequisite).prepared
    }

    /// Forward `event` to the outbound channel right away when its
    /// `prerequisite` Eth block is locally synced AND no earlier
    /// queued event is still waiting; otherwise push it onto the
    /// pending buffer to keep ordering. Held entries are released
    /// from the front by [`Self::drain_pending_events`] once their
    /// prerequisite lands.
    pub(crate) fn try_emit_or_queue(&self, event: MalachiteEvent, prerequisite: H256) {
        let mut queue = self.pending_events.lock().expect("pending_events poisoned");
        if queue.is_empty() && self.prerequisite_satisfied(prerequisite) {
            // Channel receiver dropped only on shutdown — best-effort.
            let _ = self.event_tx.send(Ok(event));
        } else {
            queue.push_back(PendingEvent {
                event,
                prerequisite,
            });
        }
    }

    /// Pop and emit pending events from the front while their
    /// prerequisite is satisfied. Stops at the first still-blocked
    /// entry so ordering is preserved (later events may have a
    /// later prerequisite, but FIFO drain only releases what's
    /// safely ready right now).
    pub(crate) fn drain_pending_events(&self) {
        let mut queue = self.pending_events.lock().expect("pending_events poisoned");
        while let Some(front) = queue.front() {
            if !self.prerequisite_satisfied(front.prerequisite) {
                break;
            }
            let entry = queue.pop_front().expect("just peeked");
            let _ = self.event_tx.send(Ok(entry.event));
        }
    }

    // Wait for either a new EB candidate past quarantine
    // or any suitable injected tx to include in the next proposal.
    async fn wait_for_proposable_content(
        &self,
        prev_advanced_eb_hash: H256,
    ) -> (Option<H256>, Vec<SignedInjectedTransaction>) {
        loop {
            let chain_head_notified = self.chain_head_notify.notified();
            tokio::pin!(chain_head_notified);
            chain_head_notified.as_mut().enable();

            let advance = self.find_eb_candidate_for_advancing(prev_advanced_eb_hash);

            let head_snapshot = *self.chain_head.read().expect("chain_head poisoned");
            let injected = match head_snapshot {
                Some(head) => self.mempool.fetch(head).await,
                None => Vec::new(),
            };

            if advance.is_some() || !injected.is_empty() {
                return (advance, injected);
            }

            tokio::select! {
                biased;
                _ = chain_head_notified => {}
                _ = self.mempool.wait_for_new_tx() => {}
            }
        }
    }

    // Candidate EB must be anchored in the quarantine and a strict descendant of the previously advanced EB.
    fn find_eb_candidate_for_advancing(&self, prev_advanced_eb_hash: H256) -> Option<H256> {
        let head = (*self.chain_head.read().expect("chain_head poisoned"))?;
        let start = self.db.globals().start_block_hash;
        // Producer-side total depth: protocol-required `canonical_quarantine`
        // plus `post_quarantine_delay` slack so validators have a fresh
        // enough local view by the time they see this MB.
        let total_depth = self.canonical_quarantine as u32 + self.post_quarantine_delay;
        let candidate = match quarantine::anchor(&self.db, head, total_depth, start) {
            Ok(Some(c)) => c,
            Ok(None) => return None,
            Err(e) => {
                warn!(error = %e, "anchor lookup failed; skipping advance");
                return None;
            }
        };
        if candidate == prev_advanced_eb_hash {
            return None;
        }
        match quarantine::is_strict_descendant_of(&self.db, candidate, prev_advanced_eb_hash, start)
        {
            Ok(true) => Some(candidate),
            Ok(false) => None,
            Err(e) => {
                error!(
                    error = %e,
                    candidate = %candidate,
                    parent_advanced = %prev_advanced_eb_hash,
                    "quarantine-passed EB is not a canonical descendant of \
                     parent's last_advanced_eb — skipping AdvanceTillEthereumBlock"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmptyMempool, MalachiteEvent};
    use ethexe_common::{
        BlockHeader,
        db::{BlockMetaStorageRW, OnChainStorageRW},
        injected::PurgedTransaction,
        malachite::{ProcessQueuesLimits, ProgressTasksLimits},
    };

    /// Build a small ethexe `Database`-backed externalities + the
    /// matching event receiver. No ethexe-malachite-core or libp2p involved —
    /// callbacks are invoked directly so we can assert on side
    /// effects deterministically.
    fn make_externalities(
        db: Database,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    ) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db,
            mempool: Arc::new(EmptyMempool),
            chain_head: Arc::new(RwLock::new(None)),
            chain_head_notify: Arc::new(Notify::new()),
            event_tx,
            pending_events: Mutex::new(VecDeque::new()),
            gas_allowance: 1_000_000,
            canonical_quarantine: 0,
            post_quarantine_delay: 0,
        };
        (ext, event_rx)
    }

    /// Build a [`Transactions`] for unit tests.
    ///
    /// The `salt` byte is encoded as the number of leading
    /// `ProgressTasks` placeholders, which gives each block a unique
    /// hash without dragging an extraneous `AdvanceTillEthereumBlock`
    /// through the test (the `last_advanced_eb_propagates` case
    /// would otherwise see an unintended advance).
    fn payload(advance: Option<H256>, salt: u8) -> Transactions {
        let mut txs = Vec::with_capacity(salt as usize + 3);
        if let Some(eth) = advance {
            txs.push(Transaction::AdvanceTillEthereumBlock { block_hash: eth });
        }
        // Salt = number of repeated ProgressTasks. Salt 0 is illegal
        // (collides with another zero-salt block); the helpers below
        // always pass salt >= 1.
        for _ in 0..(salt.max(1)) {
            txs.push(Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            });
        }
        txs.push(Transaction::ProcessQueues {
            limits: ProcessQueuesLimits::default(),
        });
        Transactions::new(txs)
    }

    fn wrap(
        payload: Transactions,
        height: u64,
        parent_hash: H256,
    ) -> ethexe_malachite_core::Block<Transactions> {
        ethexe_malachite_core::Block::<Transactions>::new(parent_hash, height, payload)
    }

    fn fake_cert(height: u64) -> ethexe_malachite_core::CommitCertificate {
        ethexe_malachite_core::CommitCertificate {
            height,
            block_hash: H256::zero(), // unused by process_mb_finalized
            signatures: vec![vec![0u8; 64]],
        }
    }

    /// `process_mb_proposal` populates `mb_block`, `mb_meta` (height,
    /// parent_mb_hash, last_advanced_eb, synced=true) and the
    /// height index, then emits a `BlockProposal`.
    #[tokio::test]
    async fn process_mb_proposal_populates_db_and_emits_event() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 1);
        let block = wrap(p.clone(), 1, H256::zero());
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();

        let compact = db.mb_compact_block(mb_hash).expect("CompactMb saved");
        assert_eq!(compact.height, 1);
        assert_eq!(compact.parent, H256::zero());
        let txs = db
            .transactions(compact.transactions_hash)
            .expect("transactions in CAS");
        assert_eq!(txs, p);

        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockProposal {
                height,
                mb_hash: proposed,
            } => {
                assert_eq!(height, 1);
                assert_eq!(proposed, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockProposal, got {other:?}"),
        }

        // Globals not advanced by save — finalize is what does that.
        assert!(db.globals().latest_finalized_mb_hash.is_zero());
    }

    /// `process_mb_finalized` reads the [`CompactMb`] +
    /// transactions blob keyed by the consensus envelope hash,
    /// advances `globals.latest_finalized_mb_hash`, and emits a
    /// `BlockFinalized`.
    #[tokio::test]
    async fn finalize_advances_globals_and_emits_event() {
        use ethexe_common::db::GlobalsStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 5);
        let block = wrap(p.clone(), 1, H256::zero());
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();
        let _ = rx.recv().await; // BlockProposal
        ext.process_mb_finalized(mb_hash, fake_cert(1))
            .await
            .unwrap();
        assert_eq!(db.globals().latest_finalized_mb_hash, mb_hash);
        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockFinalized {
                cert,
                height,
                mb_hash: finalized,
            } => {
                assert_eq!(height, 1);
                assert_eq!(mb_hash, finalized);
                assert_eq!(cert.height, 1);
                assert_eq!(cert.mb_hash, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockFinalized, got {other:?}"),
        }
    }

    /// Crash-recovery: build externalities A on a fresh DB, save +
    /// finalize K MBs, drop A, build externalities B on the same DB.
    /// B sees the persisted globals and `CompactMb` chain; the
    /// next `process_mb_proposal` correctly chains off the previous head.
    #[tokio::test]
    async fn restart_continues_from_persisted_chain() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext_a, mut rx_a) = make_externalities(db.clone());

        let mut chain: Vec<(H256, Transactions)> = Vec::new();
        let mut parent = H256::zero();
        for i in 1..=3u64 {
            let p = payload(None, i as u8);
            let block = wrap(p.clone(), i, parent);
            let mb_hash = block.hash();
            ext_a.process_mb_proposal(mb_hash, block).await.unwrap();
            ext_a
                .process_mb_finalized(mb_hash, fake_cert(i))
                .await
                .unwrap();
            chain.push((mb_hash, p));
            parent = mb_hash;
        }
        // Drain events so the channel doesn't hold stale references.
        while rx_a.try_recv().is_ok() {}
        drop(ext_a);
        drop(rx_a);

        // After "restart" — fresh externalities on the SAME DB.
        let (ext_b, mut rx_b) = make_externalities(db.clone());

        // Pre-restart pointers must survive.
        let last_pre = chain.last().unwrap().0;
        assert_eq!(db.globals().latest_finalized_mb_hash, last_pre);
        for (i, (mb_hash, _)) in chain.iter().enumerate() {
            let compact = db.mb_compact_block(*mb_hash).expect("compact");
            assert_eq!(compact.height, (i + 1) as u64);
            let expected_parent = if i == 0 { H256::zero() } else { chain[i - 1].0 };
            assert_eq!(compact.parent, expected_parent);
        }

        // Save + finalize MB at height 4 chained off the head — the
        // parent resolution must see the height-3 record left by the
        // previous run.
        let p4 = payload(None, 99);
        let block4 = wrap(p4.clone(), 4, last_pre);
        let mb4 = block4.hash();
        ext_b.process_mb_proposal(mb4, block4).await.unwrap();
        let _ = rx_b.recv().await; // proposal
        ext_b.process_mb_finalized(mb4, fake_cert(4)).await.unwrap();
        assert_eq!(db.mb_compact_block(mb4).unwrap().parent, last_pre);
        assert_eq!(db.globals().latest_finalized_mb_hash, mb4);
    }

    /// `last_advanced_eb` is propagated forward: an MB without an
    /// `AdvanceTillEthereumBlock` inherits the parent's value; an MB
    /// with one resets it.
    #[tokio::test]
    async fn last_advanced_eb_propagates() {
        use ethexe_common::db::MbStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());

        let mut chain: Vec<H256> = Vec::new();
        let mut parent = H256::zero();
        let payloads = [
            payload(None, 1),
            payload(Some(H256::repeat_byte(0xAB)), 2),
            payload(None, 3),
        ];
        for (i, p) in payloads.iter().enumerate() {
            let height = (i + 1) as u64;
            let block = wrap(p.clone(), height, parent);
            let mb_hash = block.hash();
            ext.process_mb_proposal(mb_hash, block).await.unwrap();
            ext.process_mb_finalized(mb_hash, fake_cert(height))
                .await
                .unwrap();
            chain.push(mb_hash);
            parent = mb_hash;
        }
        while rx.try_recv().is_ok() {}

        assert!(db.mb_meta(chain[0]).last_advanced_eb.is_zero());
        assert_eq!(
            db.mb_meta(chain[1]).last_advanced_eb,
            H256::repeat_byte(0xAB),
            "h2 should anchor to its own AdvanceTillEthereumBlock"
        );
        assert_eq!(
            db.mb_meta(chain[2]).last_advanced_eb,
            H256::repeat_byte(0xAB),
            "h3 inherits h2's anchor"
        );
    }

    /// `validate_block_above` catches double-`AdvanceTillEthereumBlock`
    /// proposals. The second `Advance` lands where `Injected*` /
    /// `ProgressTasks` would be expected, so the shape walk rejects it.
    #[tokio::test]
    async fn validate_rejects_two_advances() {
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xAA),
            },
            Transaction::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xBB),
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn validate_abstains_without_chain_head() {
        // Full-shape MB with one AdvanceTillEthereumBlock + no observer
        // head yet — the application can't verify the candidate's
        // quarantine status, so the vote is `Ok(false)` rather than `Err`.
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xCC),
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn validate_accepts_quarantine_passed_advance() {
        // canonical_quarantine = 0 in `make_externalities`, so any
        // ancestor of `head` in the local DB clears quarantine.
        let db = Database::memory();
        let chain_hashes = {
            let mut hashes = Vec::with_capacity(3);
            let mut parent = H256::zero();
            for i in 0..3 {
                let mut hb = [0u8; 32];
                hb[0] = 0x10 + i as u8;
                let hash = H256::from(hb);
                let header = BlockHeader {
                    height: i as u32,
                    timestamp: i as u64,
                    parent_hash: parent,
                };
                db.set_block_header(hash, header);
                // `validate_block_above` also checks events presence
                // for every Eth block on the advance walk — set an
                // empty vector so the gate passes.
                db.set_block_events(hash, &[]);
                db.mutate_block_meta(hash, |_| {});
                hashes.push((hash, header));
                parent = hash;
            }
            hashes
        };
        let head = ethexe_common::SimpleBlockData {
            hash: chain_hashes[2].0,
            header: chain_hashes[2].1,
        };
        let (ext, _rx) = make_externalities(db.clone());
        *ext.chain_head.write().unwrap() = Some(head);

        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: chain_hashes[1].0,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);
        assert!(
            ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    /// Stub mempool that records every `forget` argument so the test
    /// can assert which txs reached the mempool eviction path.
    #[derive(Default)]
    struct ForgetTracker {
        seen: tokio::sync::Mutex<Vec<SignedInjectedTransaction>>,
    }

    #[async_trait::async_trait]
    impl Mempool for ForgetTracker {
        fn insert(
            &self,
            _tx: SignedInjectedTransaction,
        ) -> Result<(), crate::mempool::MempoolInsertError> {
            Ok(())
        }

        fn set_chain_head(&self, _head: SimpleBlockData) -> Vec<PurgedTransaction> {
            Vec::new()
        }

        async fn fetch(&self, _head: SimpleBlockData) -> Vec<SignedInjectedTransaction> {
            Vec::new()
        }
        async fn forget(&self, committed: &[SignedInjectedTransaction]) {
            self.seen.lock().await.extend_from_slice(committed);
        }
        async fn wait_for_new_tx(&self) {
            std::future::pending().await
        }
    }

    /// `process_mb_finalized` must hand exactly the
    /// [`Transaction::Injected`] subset of the committed block to
    /// [`Mempool::forget`] (and nothing else — service txs like
    /// `ProcessQueues` stay out of the mempool round trip).
    #[tokio::test]
    async fn finalize_forgets_injected_txs() {
        use ethexe_common::{
            PrivateKey, SignedMessage, db::OnChainStorageRW, injected::InjectedTransaction,
        };
        use gprimitives::ActorId;

        let db = Database::memory();
        // Set up a single chain block so the injected txs reference a
        // valid `reference_block` — even though the stub mempool's
        // `insert` is a no-op, the value still travels through the
        // committed block intact.
        let ref_hash = H256::repeat_byte(0x42);
        let header = BlockHeader {
            height: 1,
            timestamp: 0,
            parent_hash: H256::zero(),
        };
        db.set_block_header(ref_hash, header);

        let pk = PrivateKey::random();
        let mk_tx = |salt: u8| {
            SignedMessage::create(
                pk.clone(),
                InjectedTransaction {
                    destination: ActorId::zero(),
                    payload: vec![1, 2, 3].try_into().unwrap(),
                    value: 0,
                    reference_block: ref_hash,
                    salt: vec![salt; 32].try_into().unwrap(),
                },
            )
            .unwrap()
        };
        let tx_a = mk_tx(1);
        let tx_b = mk_tx(2);

        let tracker = Arc::new(ForgetTracker::default());
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db: db.clone(),
            mempool: Arc::clone(&tracker) as Arc<dyn Mempool>,
            chain_head: Arc::new(RwLock::new(None)),
            chain_head_notify: Arc::new(Notify::new()),
            event_tx,
            pending_events: Mutex::new(VecDeque::new()),
            gas_allowance: 1_000_000,
            canonical_quarantine: 0,
            post_quarantine_delay: 0,
        };

        let payload = Transactions::new(vec![
            // service tx — must NOT show up in `forget`
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            // user tx #1 — must show up
            Transaction::Injected(tx_a.clone()),
            // service tx — must NOT
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
            // user tx #2 — must show up
            Transaction::Injected(tx_b.clone()),
        ]);
        let block = ethexe_malachite_core::Block::new(H256::zero(), 1, payload);
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();
        // Drain the BlockProposal event the save emits.
        let _ = event_rx.recv().await;
        ext.process_mb_finalized(
            mb_hash,
            ethexe_malachite_core::CommitCertificate {
                height: 1,
                block_hash: mb_hash,
                signatures: vec![],
            },
        )
        .await
        .unwrap();

        let seen = tracker.seen.lock().await.clone();
        let seen_hashes: Vec<_> = seen.iter().map(|t| t.data().to_hash()).collect();
        assert_eq!(
            seen.len(),
            2,
            "exactly two injected txs should be forgotten"
        );
        assert!(seen_hashes.contains(&tx_a.data().to_hash()));
        assert!(seen_hashes.contains(&tx_b.data().to_hash()));
    }

    // ------------------------------------------------------------------
    // Integration tests for build_block_above / validate_block_above
    // size + touched-programs caps. Adapted from master's
    // `tx_pool::tests::*` and `announces::tests::*`.
    // ------------------------------------------------------------------

    /// Build a full `EthexeExternalities` wired to a real
    /// `InjectedTxMempool` so we can exercise the producer-side filter
    /// + caps end-to-end.
    fn make_externalities_with_pool(
        db: Database,
        mempool: Arc<crate::InjectedTxMempool>,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    ) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db,
            mempool: mempool as Arc<dyn Mempool>,
            chain_head: Arc::new(RwLock::new(None)),
            chain_head_notify: Arc::new(Notify::new()),
            event_tx,
            pending_events: Mutex::new(VecDeque::new()),
            gas_allowance: 1_000_000,
            canonical_quarantine: 0,
            post_quarantine_delay: 0,
        };
        (ext, event_rx)
    }

    /// Adapted from master's `setup_announce`: creates a fresh
    /// computed MB on top of `parent_mb` whose program-states map has
    /// one entry per `destinations`, each pointing at an initialised
    /// program with sufficient executable balance.
    fn setup_mb_with_destinations(
        db: &Database,
        parent_mb: H256,
        destinations: &[gprimitives::ActorId],
    ) -> H256 {
        use crate::tx_validity::MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES;
        use ethexe_common::{
            MaybeHashOf, StateHashWithQueueSize,
            db::{CompactMb, MbStorageRW},
        };
        use ethexe_runtime_common::state::{
            ActiveProgram, MessageQueueHashWithSize, Program, ProgramState, Storage,
        };

        let transactions_hash = db.set_transactions(Transactions::new(vec![]));
        let mb_hash = H256::random();
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: parent_mb,
                height: u64::MAX / 2,
                transactions_hash,
            },
        );

        let state = ProgramState {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHashOf::empty(),
                pages_hash: MaybeHashOf::empty(),
                memory_infix: ethexe_common::gear_core::program::MemoryInfix::new(0),
                initialized: true,
            }),
            canonical_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            injected_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            waitlist_hash: MaybeHashOf::empty(),
            stash_hash: MaybeHashOf::empty(),
            mailbox_hash: MaybeHashOf::empty(),
            balance: 0,
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
        };
        let state_hash = db.write_program_state(state);
        let mut program_states = ethexe_common::ProgramStates::default();
        for dest in destinations {
            program_states.insert(
                *dest,
                StateHashWithQueueSize {
                    hash: state_hash,
                    canonical_queue_size: 0,
                    injected_queue_size: 0,
                },
            );
        }
        db.set_mb_program_states(mb_hash, program_states);
        db.mutate_mb_meta(mb_hash, |meta| meta.computed = true);
        mb_hash
    }

    fn signed_injected_tx(
        pk: &ethexe_common::PrivateKey,
        destination: gprimitives::ActorId,
        reference_block: H256,
        salt: u8,
    ) -> SignedInjectedTransaction {
        ethexe_common::SignedMessage::create(
            pk.clone(),
            ethexe_common::injected::InjectedTransaction {
                destination,
                payload: vec![1, 2, 3].try_into().unwrap(),
                value: 0,
                reference_block,
                salt: vec![salt; 32].try_into().unwrap(),
            },
        )
        .unwrap()
    }

    /// Port of master's `tx_pool::tests::test_select_for_announce`
    /// adapted to the MB world. A valid tx must surface in the
    /// produced MB; a non-zero-value tx is rejected at insert and
    /// never seen by `build_block_above`.
    #[tokio::test]
    async fn build_emits_valid_tx_and_drops_non_zero_value() {
        use ethexe_common::{
            injected::InjectedTransaction,
            mock::{BlockChain, Mock},
        };
        use gprimitives::ActorId;

        let db = Database::memory();
        let chain = BlockChain::mock(10u32).setup(&db);
        let dest = ActorId::from([1; 32]);
        let head = chain.blocks[10].to_simple();

        // Parent MB whose program_states snapshot includes `dest`.
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &[dest]);

        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        // Drive validity-window GC.
        let _ = mempool.set_chain_head(head);

        let pk = ethexe_common::PrivateKey::random();
        let valid = signed_injected_tx(&pk, dest, chain.blocks[9].hash, 0);
        let value_tx = ethexe_common::SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination: dest,
                payload: vec![].try_into().unwrap(),
                value: 100,
                reference_block: chain.blocks[9].hash,
                salt: vec![1; 32].try_into().unwrap(),
            },
        )
        .unwrap();

        mempool.insert(valid.clone()).unwrap();
        assert!(matches!(
            mempool.insert(value_tx.clone()),
            Err(crate::mempool::MempoolInsertError::NonZeroValue)
        ));
        assert_eq!(mempool.len(), 1);

        let (ext, _rx) = make_externalities_with_pool(db, mempool);
        *ext.chain_head.write().unwrap() = Some(head);

        let payload = ext.build_block_above(parent_mb).await.unwrap();
        let injected: Vec<_> = payload
            .iter()
            .filter_map(|tx| match tx {
                Transaction::Injected(t) => Some(t.data().to_hash()),
                _ => None,
            })
            .collect();
        assert_eq!(injected, vec![valid.data().to_hash()]);
    }

    /// Port of master's `tx_pool::tests::max_touched_programs`. The
    /// pool holds 50 txs to 50 distinct destinations; the parent MB's
    /// `program_states` contains MAX+1 known programs, of which the
    /// first MAX/2 + 1 are already "touched" by EB events on the
    /// advance block. The producer can add at most `MAX - initial`
    /// injected txs before hitting the cap.
    #[tokio::test]
    async fn build_caps_touched_programs() {
        use ethexe_common::{
            MAX_TOUCHED_PROGRAMS_PER_MB,
            events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
            mock::{BlockChain, Mock},
        };
        use gprimitives::{ActorId, MessageId};

        let db = Database::memory();
        // Seed events on block index 10: half-plus-one programs touched.
        let n_touched = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1) as u64;
        let mut chain = BlockChain::mock(10u32);
        chain.blocks[10].as_synced_mut().events = (0..n_touched)
            .map(|i| BlockEvent::Mirror {
                actor_id: ActorId::from(i),
                event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                    id: MessageId::zero(),
                    source: ActorId::zero(),
                    payload: vec![],
                    value: 0,
                    call_reply: false,
                }),
            })
            .collect();
        let chain = chain.setup(&db);

        // All MAX+1 destinations exist in the latest computed snapshot.
        let n_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB + 1) as u64;
        let destinations: Vec<ActorId> = (0..n_destinations).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &destinations);
        // eb_touched_programs needs latest_computed_mb_hash to find
        // known programs. Point it at the parent MB.
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let head = chain.blocks[10].to_simple();
        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        let _ = mempool.set_chain_head(head);
        let pk = ethexe_common::PrivateKey::random();
        // Push 50 txs targeting the upper half of destinations (the ones
        // NOT pre-touched by EB events).
        let push_start = MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1;
        let push_end = MAX_TOUCHED_PROGRAMS_PER_MB + 1;
        for i in push_start..push_end {
            mempool
                .insert(signed_injected_tx(
                    &pk,
                    ActorId::from(i as u64),
                    chain.blocks[9].hash,
                    i as u8,
                ))
                .unwrap();
        }

        let (ext, _rx) = make_externalities_with_pool(db.clone(), mempool);
        *ext.chain_head.write().unwrap() = Some(head);
        // Force AdvanceTillEthereumBlock so eb_touched_programs walks events.
        // The producer reads chain_head_notify to pick its advance candidate;
        // since canonical_quarantine = 0, head's parent (block 9) is a valid
        // advance.
        let payload = ext.build_block_above(parent_mb).await.unwrap();
        let advance_present = payload
            .iter()
            .any(|tx| matches!(tx, Transaction::AdvanceTillEthereumBlock { .. }));
        assert!(
            advance_present,
            "advance must be present for the EB-events touched seed to apply"
        );
        let injected_count = payload
            .iter()
            .filter(|tx| matches!(tx, Transaction::Injected(_)))
            .count();
        // Master's expectation: producer can add at most
        // `MAX - already_touched` injected destinations.
        let expected = (MAX_TOUCHED_PROGRAMS_PER_MB as usize).saturating_sub(n_touched as usize);
        assert_eq!(
            injected_count, expected,
            "expected {expected} injected txs (MAX - initial_touched), got {injected_count}",
        );
    }

    /// Port of master's `announces::tests::reject_announce_with_too_many_touched_programs`.
    /// A participant must reject an MB whose injected destinations
    /// push the touched-programs total over the cap.
    #[tokio::test]
    async fn validate_rejects_mb_with_too_many_touched_programs() {
        use ethexe_common::{
            MAX_TOUCHED_PROGRAMS_PER_MB,
            events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
            mock::{BlockChain, Mock},
        };
        use gprimitives::{ActorId, MessageId};

        let db = Database::memory();
        let n_touched = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1) as u64;
        let mut chain = BlockChain::mock(10u32);
        chain.blocks[10].as_synced_mut().events = (0..n_touched)
            .map(|i| BlockEvent::Mirror {
                actor_id: ActorId::from(i),
                event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                    id: MessageId::zero(),
                    source: ActorId::zero(),
                    payload: vec![],
                    value: 0,
                    call_reply: false,
                }),
            })
            .collect();
        let chain = chain.setup(&db);

        let n_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB + 1) as u64;
        let destinations: Vec<ActorId> = (0..n_destinations).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &destinations);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let head = chain.blocks[10].to_simple();
        let (ext, _rx) = make_externalities(db.clone());
        *ext.chain_head.write().unwrap() = Some(head);

        // Craft an MB payload that adds N/2 fresh destinations on top
        // of the N/2+1 already touched by EB events → total > limit.
        // Advance to block 10 so `eb_touched_programs` walks the
        // events block we just seeded.
        let advance_block = chain.blocks[10].hash;
        let pk = ethexe_common::PrivateKey::random();
        let extra_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1
            ..MAX_TOUCHED_PROGRAMS_PER_MB + 1)
            .map(|i| ActorId::from(i as u64));
        let mut transactions = vec![Transaction::AdvanceTillEthereumBlock {
            block_hash: advance_block,
        }];
        for (i, dest) in extra_destinations.enumerate() {
            transactions.push(Transaction::Injected(signed_injected_tx(
                &pk,
                dest,
                chain.blocks[9].hash,
                i as u8,
            )));
        }
        // Full shape — the shape walk must not be the reason for rejection.
        transactions.push(Transaction::ProgressTasks {
            limits: ProgressTasksLimits::default(),
        });
        transactions.push(Transaction::ProcessQueues {
            limits: ProcessQueuesLimits::default(),
        });
        let payload = Transactions::new(transactions);
        assert!(
            !ext.validate_block_above(parent_mb, payload).await.unwrap(),
            "MB must be rejected when touched destinations + EB-touched > cap"
        );
    }

    /// Port of master's idea: a fully-sized batch of injected txs
    /// must trip the per-MB size cap. We feed the pool enough txs that
    /// their cumulative encoded size exceeds
    /// `MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB`; the producer must
    /// emit only as many as fit.
    #[tokio::test]
    async fn build_caps_total_encoded_size() {
        use ethexe_common::{
            injected::{
                InjectedTransaction, MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB,
                MAX_INJECTED_TX_PAYLOAD_SIZE,
            },
            mock::{BlockChain, Mock},
        };
        use gprimitives::ActorId;
        use parity_scale_codec::Encode;

        let db = Database::memory();
        let chain = BlockChain::mock(2u32).setup(&db);
        let head = chain.blocks[2].to_simple();
        // Many destinations so the touched-programs cap can't be the
        // limiting factor (one destination per tx).
        let dests: Vec<ActorId> = (0..16u64).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(1), &dests);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        let _ = mempool.set_chain_head(head);
        let pk = ethexe_common::PrivateKey::random();
        // Each tx carries the maximum-size payload; the pool is loaded
        // with enough of them that two fit but three don't.
        for (i, dest) in dests.iter().enumerate().take(3) {
            let tx = ethexe_common::SignedMessage::create(
                pk.clone(),
                InjectedTransaction {
                    destination: *dest,
                    payload: vec![0u8; MAX_INJECTED_TX_PAYLOAD_SIZE / 2]
                        .try_into()
                        .unwrap(),
                    value: 0,
                    reference_block: chain.blocks[1].hash,
                    salt: vec![i as u8; 32].try_into().unwrap(),
                },
            )
            .unwrap();
            mempool.insert(tx).unwrap();
        }
        assert_eq!(mempool.len(), 3);

        let (ext, _rx) = make_externalities_with_pool(db.clone(), mempool);
        *ext.chain_head.write().unwrap() = Some(head);

        let payload = ext.build_block_above(parent_mb).await.unwrap();
        let injected: Vec<_> = payload
            .iter()
            .filter_map(|tx| match tx {
                Transaction::Injected(t) => Some(t.encoded_size()),
                _ => None,
            })
            .collect();
        let total: usize = injected.iter().sum();
        assert!(
            total <= MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB,
            "cumulative encoded size ({total}) exceeds per-MB cap ({MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB})",
        );
        // With 3 half-payload txs each ~64 KiB, only the first two should
        // fit under the 127 KiB cap. (Each encoded tx adds ~64 KiB + envelope.)
        assert!(
            injected.len() < 3,
            "size cap must drop at least one tx, got {} retained",
            injected.len()
        );
    }

    // ------------------------------------------------------------------
    // Shape & ordering checks on `validate_block_above`.
    //
    // Every MB the producer emits has the strict shape
    //   [AdvanceTillEthereumBlock]?  Injected*  ProgressTasks  ProcessQueues
    // with `ProcessQueues.limits.gas_allowance <= DEFAULT_GAS_ALLOWANCE`.
    // A malicious proposer must not be able to slip in a malformed MB
    // (oversized gas, missing bookend, out-of-order tx).
    // ------------------------------------------------------------------

    /// Helper: build a tiny chain with one block past quarantine and an
    /// `EthexeExternalities` whose chain_head points at it. Returns the
    /// ext + the advance block hash to use for `AdvanceTillEthereumBlock`.
    fn chain_with_one_advance(
        db: Database,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
        H256,
    ) {
        let mut parent = H256::zero();
        let mut chain_hashes = Vec::new();
        for i in 0..3u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x10 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain_hashes.push((hash, header));
            parent = hash;
        }
        let head = ethexe_common::SimpleBlockData {
            hash: chain_hashes[2].0,
            header: chain_hashes[2].1,
        };
        let advance_hash = chain_hashes[1].0;
        let (ext, rx) = make_externalities(db);
        *ext.chain_head.write().unwrap() = Some(head);
        (ext, rx, advance_hash)
    }

    /// REPRODUCES: a malicious proposer can set `gas_allowance = u64::MAX`
    /// in `ProcessQueues.limits` and force every participant to attempt
    /// an unbounded queue drain. Validator must reject MBs whose
    /// `gas_allowance` exceeds the protocol cap
    /// (`MalachiteConfig::DEFAULT_GAS_ALLOWANCE`).
    #[tokio::test]
    async fn validate_rejects_gas_allowance_above_default() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db);
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits {
                    gas_allowance: u64::MAX,
                },
            },
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap(),
            "MB with `gas_allowance > DEFAULT_GAS_ALLOWANCE` must be rejected"
        );
    }

    /// REPRODUCES: MB without a `ProgressTasks` tx between injected txs
    /// and `ProcessQueues` violates the protocol shape — scheduled
    /// tasks would never be advanced.
    #[tokio::test]
    async fn validate_rejects_mb_missing_progress_tasks() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db);
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            // No ProgressTasks here — straight to ProcessQueues.
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap(),
            "MB missing `ProgressTasks` bookend must be rejected"
        );
    }

    /// REPRODUCES: MB without a final `ProcessQueues` tx never drains
    /// the message queues for this MB — compute pipeline would stall
    /// on the next MB.
    #[tokio::test]
    async fn validate_rejects_mb_missing_process_queues() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db);
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            // No ProcessQueues here.
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap(),
            "MB missing `ProcessQueues` bookend must be rejected"
        );
    }

    /// REPRODUCES: `AdvanceTillEthereumBlock` must be the very first tx
    /// in the MB. Otherwise EB events are not the first action of the
    /// MB and compute pipeline runs queues against stale state.
    #[tokio::test]
    async fn validate_rejects_advance_not_first() {
        use ethexe_common::{
            PrivateKey, SignedMessage,
            injected::InjectedTransaction,
            mock::{BlockChain, Mock},
        };

        let db = Database::memory();
        let chain = BlockChain::mock(2u32).setup(&db);
        let head = chain.blocks[2].to_simple();
        let dest = gprimitives::ActorId::from([1; 32]);
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(1), &[dest]);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let (ext, _rx) = make_externalities(db.clone());
        *ext.chain_head.write().unwrap() = Some(head);

        let pk = PrivateKey::random();
        let tx = SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination: dest,
                payload: vec![].try_into().unwrap(),
                value: 0,
                reference_block: chain.blocks[1].hash,
                salt: vec![7; 32].try_into().unwrap(),
            },
        )
        .unwrap();
        let payload = Transactions::new(vec![
            // Order swapped: Injected before Advance.
            Transaction::Injected(tx),
            Transaction::AdvanceTillEthereumBlock {
                block_hash: chain.blocks[2].hash,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);
        assert!(
            !ext.validate_block_above(parent_mb, payload).await.unwrap(),
            "MB where Advance is not the first tx must be rejected"
        );
    }

    /// REPRODUCES: `ProcessQueues` must be the very last tx in the MB.
    /// Otherwise later txs run *after* queues drain and the gas budget
    /// is wrong.
    #[tokio::test]
    async fn validate_rejects_process_queues_not_last() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db);
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            // Order swapped: ProcessQueues before ProgressTasks.
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap(),
            "MB where `ProcessQueues` is not the last tx must be rejected"
        );
    }

    #[tokio::test]
    async fn validate_rejects_advance_that_regresses_last_advanced_eb() {
        let db = Database::memory();

        // Build a 5-block chain.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..5u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x10 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        let head = ethexe_common::SimpleBlockData {
            hash: chain[4].0,
            header: chain[4].1,
        };

        // Seed a parent MB whose `last_advanced_eb` points at chain[3]
        // (a relatively recent EB). The proposer's `advance` then points
        // at chain[1] (regressing). Both pass quarantine (depth = 1+ vs.
        // `canonical_quarantine = 0`), but chain[1] is a strict ancestor
        // of chain[3], so the descendant check would reject — and that
        // is exactly what we want validators to do.
        let parent_mb = H256::from([0xCD; 32]);
        let transactions_hash = db.set_transactions(Transactions::new(vec![]));
        db.set_mb_compact_block(
            parent_mb,
            ethexe_common::db::CompactMb {
                parent: H256::zero(),
                height: 1,
                transactions_hash,
            },
        );
        db.set_mb_program_states(parent_mb, ethexe_common::ProgramStates::default());
        db.mutate_mb_meta(parent_mb, |meta| {
            meta.computed = true;
            meta.last_advanced_eb = chain[3].0;
        });

        let (ext, _rx) = make_externalities(db.clone());
        *ext.chain_head.write().unwrap() = Some(head);

        // MB proposes Advance to chain[1] — strictly older than chain[3]
        // (parent's last_advanced_eb). `verify_passed` accepts (chain[1]
        // is a canonical ancestor of head); `is_strict_descendant_of`
        // would reject (chain[1] does NOT descend from chain[3]).
        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: chain[1].0,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);

        assert!(
            !ext.validate_block_above(parent_mb, payload).await.unwrap(),
            "MB whose AdvanceTillEthereumBlock regresses parent.last_advanced_eb \
             must be rejected — currently passes because validate_block_above \
             skips the strict-descendant check the producer enforces",
        );
    }

    /// `validate_block_above` must return synchronously when the local
    /// view does not yet cover the proposer's `advance`. Previously it
    /// polled in a 2-second loop; now it abstains immediately so the
    /// engine can rotate to the next proposer. The 50 ms timeout below
    /// is *much* shorter than the old 2 s poll budget — if the function
    /// still waited, the timeout would fire.
    #[tokio::test]
    async fn validate_rejects_advance_when_chain_head_does_not_cover_it() {
        let db = Database::memory();

        // Build a small canonical chain `[c0, c1, c2]`.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..3u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x20 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        // Local chain_head = c1 (lagging behind the proposer's view).
        let head = ethexe_common::SimpleBlockData {
            hash: chain[1].0,
            header: chain[1].1,
        };

        // Proposer's `advance` points at a block our DB has no
        // canonical-ancestor record for from `head` — a fully
        // unrelated hash. `verify_passed` will return Err and
        // validation must reject in one shot.
        let stranger_advance = H256::from([0xEE; 32]);

        let (ext, _rx) = make_externalities(db.clone());
        *ext.chain_head.write().unwrap() = Some(head);

        let payload = Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: stranger_advance,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ]);

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            ext.validate_block_above(H256::zero(), payload),
        )
        .await
        .expect("validate_block_above must return synchronously, not wait on local sync");
        assert!(
            !result.unwrap(),
            "MB with an advance the local view doesn't cover must be rejected"
        );
    }

    /// Producer-side: `find_eb_candidate_for_advancing` picks the
    /// anchor at depth `canonical_quarantine + post_quarantine_delay`
    /// from the local chain head. With `(canonical_quarantine = 2,
    /// post_quarantine_delay = 3)` the candidate sits 5 blocks below
    /// head — exactly what we verify by walking parent links.
    #[test]
    fn producer_picks_anchor_at_canonical_plus_post_delay() {
        use ethexe_common::db::OnChainStorageRO;

        let db = Database::memory();

        // Long chain — `canonical_quarantine + post_quarantine_delay`
        // must walk back 5 blocks, so we need at least 7 to leave
        // headroom and confirm the walk stops at the right depth.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..8u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x30 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        // `start_block_hash = chain[0]` keeps the fence at genesis so it
        // never trips for this walk.
        db.globals_mutate(|g| g.start_block_hash = chain[0].0);

        let head_idx = chain.len() - 1;
        let head = ethexe_common::SimpleBlockData {
            hash: chain[head_idx].0,
            header: chain[head_idx].1,
        };

        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db: db.clone(),
            mempool: Arc::new(EmptyMempool),
            chain_head: Arc::new(RwLock::new(Some(head))),
            chain_head_notify: Arc::new(Notify::new()),
            event_tx,
            pending_events: Mutex::new(VecDeque::new()),
            gas_allowance: 1_000_000,
            canonical_quarantine: 2,
            post_quarantine_delay: 3,
        };

        let candidate = ext
            .find_eb_candidate_for_advancing(H256::zero())
            .expect("must surface a candidate — chain is deep enough");
        // Walk back `2 + 3 = 5` parents from head; that's the expected
        // anchor.
        let mut cursor = head.hash;
        for _ in 0..5 {
            let h = db
                .block_header(cursor)
                .expect("test chain headers are populated");
            cursor = h.parent_hash;
        }
        assert_eq!(
            candidate, cursor,
            "anchor must sit at depth `canonical_quarantine + post_quarantine_delay` from head",
        );
        let candidate_height = db.block_header(candidate).unwrap().height;
        assert_eq!(
            candidate_height,
            chain[head_idx].1.height - 5,
            "depth arithmetic mismatch — expected exactly 5 blocks below head",
        );
    }
}
