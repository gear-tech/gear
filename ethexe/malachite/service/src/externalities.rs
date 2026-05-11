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

//! [`ethexe_malachite_core::Externalities`] glue for ethexe.
//!
//! ethexe-malachite-core is application-agnostic — it owns the BFT engine, the
//! libp2p swarm, and the persistent consensus state. Everything
//! ethexe-specific (block contents, validation rules, DB schema)
//! lives behind this trait.
//!
//! ## Map of responsibilities
//! - [`EthexeExternalities::save_block`] — once ethexe-malachite-core agrees an MB
//!   is saveable (parent already saved), persist it to the ethexe
//!   `mb_*` keyspace, propagate `last_advanced_block`, and fire
//!   [`MalachiteEvent::BlockProposal`].
//! - [`EthexeExternalities::mark_block_as_finalized`] — flush the
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
//! [`EthexeExternalities::save_block`] writes a [`CompactBlock`] under
//! that key (carrying parent + height + the Blake2b hash of the
//! [`Transactions`] payload) and CAS-stores the `Transactions` blob;
//! [`EthexeExternalities::mark_block_as_finalized`] reads both back
//! via the same key the consensus layer hands in.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{
    SimpleBlockData,
    db::{
        CompactBlock, GlobalsStorageRO, GlobalsStorageRW, MbStorageRO, MbStorageRW,
        OnChainStorageRO,
    },
    injected::SignedInjectedTransaction,
    mb::{ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions},
};
use ethexe_db::Database;
use gprimitives::H256;
use tokio::sync::{Notify, mpsc};
use tracing::{error, info, warn};

use crate::{CommitCertificate, MalachiteEvent, Mempool, quarantine};

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
    /// Decoupled from `globals.latest_synced_block` because the latter
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
    /// `last_advanced_block` Eth-block isn't fully synced into the
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
}

/// One outbound [`MalachiteEvent`] that can't be released until its
/// `prerequisite` Eth block is fully synced into the local DB.
pub(crate) struct PendingEvent {
    pub event: MalachiteEvent,
    /// Eth-block hash whose `block_events` entry must be present
    /// before this event can fire — i.e. the MB's
    /// `last_advanced_block`. `H256::zero()` skips the gate (genesis
    /// or an MB that never advanced past the pre-genesis sentinel).
    pub prerequisite: H256,
}

#[async_trait]
impl ethexe_malachite_core::Externalities<Transactions> for EthexeExternalities {
    async fn save_block(
        &self,
        block_hash: H256,
        block: ethexe_malachite_core::Block<Transactions>,
    ) -> Result<()> {
        // The DB is keyed by the consensus envelope hash (Blake2b
        // over `Block`), passed in `block_hash`. Parent linkage lives
        // in [`CompactBlock::parent`]; the transactions list itself
        // lives in CAS keyed by [`CompactBlock::transactions_hash`].
        let parent = block.parent_hash;
        let payload = block.payload;

        // Propagate `last_advanced_block` forward — the latest
        // `AdvanceTillEthereumBlock` in this MB wins; otherwise we
        // inherit the parent's value (zero if pre-genesis).
        let parent_advanced = if parent.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent).last_advanced_block
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
        // CompactBlock exists, transactions are reachable" — holds
        // unconditionally.
        let transactions_hash = self.db.set_transactions(payload.clone());
        self.db.set_mb_compact_block(
            block_hash,
            CompactBlock {
                parent,
                height: block.height,
                transactions_hash,
            },
        );
        self.db.mutate_mb_meta(block_hash, |meta| {
            meta.last_advanced_block = last_advanced;
            // ethexe-malachite-core's ancestor-first ordering means
            // the chain back to genesis is intact by the time
            // `save_block` fires.
            meta.synced = true;
        });

        self.try_emit_or_queue(
            MalachiteEvent::BlockProposal {
                height: block.height,
                block_hash,
            },
            last_advanced,
        );
        Ok(())
    }

    async fn mark_block_as_finalized(
        &self,
        block_hash: H256,
        cert: ethexe_malachite_core::CommitCertificate,
    ) -> Result<()> {
        let compact = self.db.mb_compact_block(block_hash).ok_or_else(|| {
            anyhow!("mark_finalized: no CompactBlock for {block_hash} (save_block must run first)")
        })?;
        let payload = self
            .db
            .transactions(compact.transactions_hash)
            .ok_or_else(|| {
                anyhow!(
                    "mark_finalized: transactions blob {} missing for block {block_hash}",
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
            .globals_mutate(|g| g.latest_finalized_mb_hash = block_hash);

        let app_cert = CommitCertificate {
            height: cert.height,
            block_hash,
            signatures: cert.signatures,
        };
        // Same prerequisite as the matching BlockProposal — by the
        // time `mark_block_as_finalized` runs, `save_block` has
        // already populated `mb_meta(block_hash).last_advanced_block`.
        let last_advanced = self.db.mb_meta(block_hash).last_advanced_block;
        self.try_emit_or_queue(
            MalachiteEvent::BlockFinalized {
                cert: app_cert,
                height: cert.height,
                block_hash,
            },
            last_advanced,
        );
        Ok(())
    }

    async fn build_block_above(&self, parent_hash: H256) -> Result<Transactions> {
        // `parent_hash` is the consensus envelope hash of the parent
        // (zero for genesis). Use it directly to seed the producer's
        // `last_advanced_block` lookup.
        let parent_advanced = if parent_hash.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_hash).last_advanced_block
        };

        let (advance, injected) = self.wait_for_proposable_content(parent_advanced).await;

        info!(
            %parent_hash,
            %parent_advanced,
            advance = ?advance,
            injected_count = injected.len(),
            "build_block_above: proposable content resolved",
        );

        // Producer pacing:
        //   1. AdvanceTillEthereumBlock first (if a fresh
        //      quarantine-passed EB exists),
        //   2. then injected user txs,
        //   3. finally the service-level ProgressTasks +
        //      ProcessQueues bookend.
        let mut transactions = Vec::with_capacity(injected.len() + 3);
        if let Some(block_hash) = advance {
            transactions.push(Transaction::AdvanceTillEthereumBlock { block_hash });
        }
        for tx in injected {
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
        // Parent linkage and height progression are validated by
        // ethexe-malachite-core itself; here we only check the
        // payload-level invariants.

        // (1) At most one AdvanceTillEthereumBlock per MB. Zero is
        // legal (chain still too close to genesis); two+ is a
        // protocol violation.
        let advances: Vec<H256> = payload
            .iter()
            .filter_map(|tx| match tx {
                Transaction::AdvanceTillEthereumBlock { block_hash } => Some(*block_hash),
                _ => None,
            })
            .collect();
        if advances.len() > 1 {
            warn!(
                count = advances.len(),
                "validate: more than one AdvanceTillEthereumBlock — rejecting"
            );
            return Ok(false);
        }
        let Some(advance) = advances.first().copied() else {
            return Ok(true);
        };

        // (2) Quarantine + local-sync — wait briefly for the local
        // observer to catch up if the proposer raced ahead.
        //
        // The proposer was likely 1 Hoodi block ahead of us when it
        // built this proposal: its anchor (`head - canonical_quarantine`)
        // sits one block too shallow from our local head's POV, so a
        // strict synchronous check would prevote nil and force the
        // round to time out (≥ propose_timeout). Instead we poll —
        // every chain_head update or up to a hard deadline — and
        // succeed as soon as our DB covers the proposer's advance.
        //
        // The deadline is intentionally well below the engine's
        // protocol-level propose timeout: if we still can't validate
        // by then, the proposer's chain genuinely diverges from ours
        // and prevoting nil is the correct outcome.
        let parent_advanced = if parent_hash.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_hash).last_advanced_block
        };

        const VALIDATE_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_millis(2000);
        const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
        let deadline = tokio::time::Instant::now() + VALIDATE_WAIT_BUDGET;
        let start_block_hash = self.db.globals().start_block_hash;

        loop {
            let head_opt = *self.chain_head.read().expect("chain_head poisoned");
            if let Some(head) = head_opt {
                let quarantine_ok = quarantine::verify_passed(
                    &self.db,
                    head,
                    advance,
                    self.canonical_quarantine,
                    start_block_hash,
                );
                let sync_ok = self.advance_chain_locally_synced(advance, parent_advanced);
                if quarantine_ok.is_ok() && sync_ok {
                    return Ok(true);
                }
                // Past deadline: log the still-failing reason and give up.
                if tokio::time::Instant::now() >= deadline {
                    if let Err(e) = quarantine_ok {
                        warn!(
                            error = %e,
                            %advance,
                            head = %head.hash,
                            head_height = head.header.height,
                            "validate: quarantine still not satisfied within budget — abstaining",
                        );
                    } else {
                        warn!(
                            %advance,
                            %parent_advanced,
                            head = %head.hash,
                            "validate: advance-chain not yet locally synced — abstaining",
                        );
                    }
                    return Ok(false);
                }
            } else if tokio::time::Instant::now() >= deadline {
                warn!("validate: no chain-head event yet — abstaining from vote");
                return Ok(false);
            }

            // Poll the local view periodically. The observer pumps
            // a fresh chain_head into us asynchronously, so within a
            // few hundred milliseconds the local DB is up-to-date
            // and the next iteration of this loop succeeds. This
            // avoids the older synchronous-prevote-nil path that
            // forced rounds to time out at 13 s whenever the
            // proposer was 1 Hoodi block ahead of us.
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

impl EthexeExternalities {
    /// True iff `prerequisite.is_zero()` (no prerequisite — genesis
    /// or pre-advance) or its events are already in the local DB.
    /// Observer-side `BlockSynced` populates `block_events` after
    /// the full ancestor chain is in place, so this is exactly the
    /// "downstream `compute_mb` won't trip on a missing header"
    /// condition.
    fn prerequisite_satisfied(&self, prerequisite: H256) -> bool {
        prerequisite.is_zero() || self.db.block_events(prerequisite).is_some()
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

    /// Block until either a quarantine-passed EB advance is available
    /// or the mempool has injected txs whose `reference_block` is on
    /// the local canonical chain. Returns the (advance, injected)
    /// pair already pre-resolved so the caller doesn't double-fetch.
    ///
    /// Re-evaluates on every chain-head update or mempool insert so
    /// the producer never waits on stale state.
    async fn wait_for_proposable_content(
        &self,
        parent_advanced: H256,
    ) -> (Option<H256>, Vec<SignedInjectedTransaction>) {
        loop {
            // Arm the chain-head notification BEFORE checking conditions.
            // `Notify::notify_waiters()` only wakes futures that are already
            // registered as waiters; without pre-arming, a wake fired
            // between `compute_advance_candidate` and `select!` is lost
            // and the producer hangs until `propose_timeout` (12s) — a
            // showstopper for tests that mine one block at a time.
            let chain_head_notified = self.chain_head_notify.notified();
            tokio::pin!(chain_head_notified);
            chain_head_notified.as_mut().enable();

            let advance = self.compute_advance_candidate(parent_advanced);
            // Snapshot the chain head and drop the guard before the
            // mempool's async fetch — the guard is `!Send`, so any
            // await across the lock would poison the impl Trait future.
            let head_snapshot = *self.chain_head.read().expect("chain_head poisoned");
            let injected = match head_snapshot {
                Some(head) => self.mempool.fetch(head, self.gas_allowance).await,
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

    /// Resolve the next `AdvanceTillEthereumBlock` candidate given
    /// the parent MB's `last_advanced_block`. Returns `Some` only for
    /// a strict descendant of `parent_advanced`; everything else
    /// (no candidate, same EB, or a misconfigured walk) is treated
    /// as "no advance this round" and logged.
    fn compute_advance_candidate(&self, parent_advanced: H256) -> Option<H256> {
        let head = (*self.chain_head.read().expect("chain_head poisoned"))?;
        let start = self.db.globals().start_block_hash;
        let candidate = match quarantine::anchor(&self.db, head, self.canonical_quarantine, start) {
            Ok(Some(c)) => c,
            Ok(None) => return None,
            Err(e) => {
                warn!(error = %e, "anchor lookup failed; skipping advance");
                return None;
            }
        };
        if candidate == parent_advanced {
            return None;
        }
        match quarantine::is_strict_descendant_of(&self.db, candidate, parent_advanced, start) {
            Ok(true) => Some(candidate),
            Ok(false) => None,
            Err(e) => {
                error!(
                    error = %e,
                    candidate = %candidate,
                    parent_advanced = %parent_advanced,
                    "quarantine-passed EB is not a canonical descendant of \
                     parent's last_advanced_block — skipping AdvanceTillEthereumBlock"
                );
                None
            }
        }
    }

    /// Return `true` iff every Eth block on the canonical walk from
    /// `advance` (inclusive) back to `parent_advanced` (exclusive) has
    /// both its header and its events present in the local DB.
    ///
    /// Mirrors the walk `ethexe_processor::Processor::collect_advance_chain`
    /// performs at execution time, but bails early instead of erroring
    /// — used by [`Self::validate_block_above`] to abstain from voting
    /// on a proposal whose required Eth state we haven't fully synced.
    /// Treated as a transient condition: subsequent rounds re-run this
    /// check after the observer makes more progress.
    fn advance_chain_locally_synced(&self, advance: H256, parent_advanced: H256) -> bool {
        if advance == parent_advanced {
            return true;
        }
        // Same safety cap as `Processor::collect_advance_chain`.
        const MAX_STEPS: u32 = 1024;
        let mut current = advance;
        for _ in 0..MAX_STEPS {
            let Some(header) = self.db.block_header(current) else {
                return false;
            };
            // BlockSynced fires only after both header and events
            // have landed; a missing events entry is the tightest
            // signal that the observer hasn't finished syncing
            // `current` yet.
            if self.db.block_events(current).is_none() {
                return false;
            }
            if current == parent_advanced {
                return true;
            }
            let parent = header.parent_hash;
            if parent.is_zero() {
                // Genesis. If we haven't yet hit `parent_advanced`,
                // either the parent chain doesn't reach it (proposal
                // is on a different fork) or `parent_advanced` is
                // also zero — handled at the top.
                return parent_advanced.is_zero();
            }
            current = parent;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmptyMempool, MalachiteEvent};
    use ethexe_common::{
        BlockHeader,
        db::{BlockMetaStorageRW, OnChainStorageRW},
        mb::{ProcessQueuesLimits, ProgressTasksLimits},
    };
    use ethexe_malachite_core::Externalities as _;

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
        };
        (ext, event_rx)
    }

    /// Build a [`Transactions`] for unit tests.
    ///
    /// The `salt` byte is encoded as the number of leading
    /// `ProgressTasks` placeholders, which gives each block a unique
    /// hash without dragging an extraneous `AdvanceTillEthereumBlock`
    /// through the test (the `last_advanced_block_propagates` case
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
            block_hash: H256::zero(), // unused by mark_block_as_finalized
            signatures: vec![vec![0u8; 64]],
        }
    }

    /// `save_block` populates `mb_block`, `mb_meta` (height,
    /// parent_mb_hash, last_advanced_block, synced=true) and the
    /// height index, then emits a `BlockProposal`.
    #[tokio::test]
    async fn save_block_populates_db_and_emits_event() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 1);
        let block = wrap(p.clone(), 1, H256::zero());
        let mb_hash = block.hash();
        ext.save_block(mb_hash, block).await.unwrap();

        let compact = db.mb_compact_block(mb_hash).expect("CompactBlock saved");
        assert_eq!(compact.height, 1);
        assert_eq!(compact.parent, H256::zero());
        let txs = db
            .transactions(compact.transactions_hash)
            .expect("transactions in CAS");
        assert_eq!(txs, p);
        let meta = db.mb_meta(mb_hash);
        assert!(meta.synced);

        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockProposal { height, block_hash } => {
                assert_eq!(height, 1);
                assert_eq!(block_hash, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockProposal, got {other:?}"),
        }

        // Globals not advanced by save — finalize is what does that.
        assert!(db.globals().latest_finalized_mb_hash.is_zero());
    }

    /// `mark_block_as_finalized` reads the [`CompactBlock`] +
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
        ext.save_block(mb_hash, block).await.unwrap();
        let _ = rx.recv().await; // BlockProposal
        ext.mark_block_as_finalized(mb_hash, fake_cert(1))
            .await
            .unwrap();
        assert_eq!(db.globals().latest_finalized_mb_hash, mb_hash);
        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockFinalized {
                cert,
                height,
                block_hash,
            } => {
                assert_eq!(height, 1);
                assert_eq!(block_hash, mb_hash);
                assert_eq!(cert.height, 1);
                assert_eq!(cert.block_hash, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockFinalized, got {other:?}"),
        }
    }

    /// Crash-recovery: build externalities A on a fresh DB, save +
    /// finalize K MBs, drop A, build externalities B on the same DB.
    /// B sees the persisted globals and `CompactBlock` chain; the
    /// next `save_block` correctly chains off the previous head.
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
            ext_a.save_block(mb_hash, block).await.unwrap();
            ext_a
                .mark_block_as_finalized(mb_hash, fake_cert(i))
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
        ext_b.save_block(mb4, block4).await.unwrap();
        let _ = rx_b.recv().await; // proposal
        ext_b
            .mark_block_as_finalized(mb4, fake_cert(4))
            .await
            .unwrap();
        assert_eq!(db.mb_compact_block(mb4).unwrap().parent, last_pre);
        assert_eq!(db.globals().latest_finalized_mb_hash, mb4);
    }

    /// `last_advanced_block` is propagated forward: an MB without an
    /// `AdvanceTillEthereumBlock` inherits the parent's value; an MB
    /// with one resets it.
    #[tokio::test]
    async fn last_advanced_block_propagates() {
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
            ext.save_block(mb_hash, block).await.unwrap();
            ext.mark_block_as_finalized(mb_hash, fake_cert(height))
                .await
                .unwrap();
            chain.push(mb_hash);
            parent = mb_hash;
        }
        while rx.try_recv().is_ok() {}

        assert!(db.mb_meta(chain[0]).last_advanced_block.is_zero());
        assert_eq!(
            db.mb_meta(chain[1]).last_advanced_block,
            H256::repeat_byte(0xAB),
            "h2 should anchor to its own AdvanceTillEthereumBlock"
        );
        assert_eq!(
            db.mb_meta(chain[2]).last_advanced_block,
            H256::repeat_byte(0xAB),
            "h3 inherits h2's anchor"
        );
    }

    /// `validate_block_above` catches double-`AdvanceTillEthereumBlock`
    /// proposals and enforces the chain-head presence requirement.
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
        ]);
        assert!(
            !ext.validate_block_above(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn validate_abstains_without_chain_head() {
        // One AdvanceTillEthereumBlock + no observer head yet — the
        // application can't verify the candidate's quarantine status,
        // so the vote is `Ok(false)` rather than `Err`.
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let payload = Transactions::new(vec![Transaction::AdvanceTillEthereumBlock {
            block_hash: H256::repeat_byte(0xCC),
        }]);
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

        let payload = Transactions::new(vec![Transaction::AdvanceTillEthereumBlock {
            block_hash: chain_hashes[1].0,
        }]);
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
        fn insert(&self, _tx: SignedInjectedTransaction) {}
        fn set_chain_head(&self, _head: SimpleBlockData) {}
        async fn fetch(
            &self,
            _head: SimpleBlockData,
            _gas_budget: u64,
        ) -> Vec<SignedInjectedTransaction> {
            Vec::new()
        }
        async fn forget(&self, committed: &[SignedInjectedTransaction]) {
            self.seen.lock().await.extend_from_slice(committed);
        }
        async fn wait_for_new_tx(&self) {
            std::future::pending().await
        }
    }

    /// `mark_block_as_finalized` must hand exactly the
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
        ext.save_block(mb_hash, block).await.unwrap();
        // Drain the BlockProposal event the save emits.
        let _ = event_rx.recv().await;
        ext.mark_block_as_finalized(
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
}
