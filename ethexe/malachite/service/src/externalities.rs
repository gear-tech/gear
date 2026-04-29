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
//!   or a non-empty mempool), then assemble a [`SequencerBlock`].
//! - [`EthexeExternalities::validate_block_above`] — for an incoming
//!   peer proposal, run ethexe's quarantine + parent-link checks
//!   before voting.
//!
//! ## Hash bridging
//! ethexe-malachite-core identifies blocks by Blake2b of its own [`Block<P>`]
//! envelope; ethexe identifies MBs by Keccak of [`SequencerBlock`].
//! We bridge the two by writing `mb_hash_at_height(height)` in
//! [`Self::save_block`] (using ethexe-malachite-core's strict ancestor-first
//! ordering, every height is recorded before its successor needs it)
//! and reading it back in [`Self::mark_block_as_finalized`] /
//! [`Self::build_block_above`]. No new key prefixes are needed; the
//! existing ethexe-db schema covers the round-trip.

use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{
    SimpleBlockData,
    db::{GlobalsStorageRO, GlobalsStorageRW, MbStorageRO, MbStorageRW},
    injected::SignedInjectedTransaction,
    mb::{ProcessQueuesLimits, ProgressTasksLimits, SequencerBlock, Transaction},
};
use ethexe_db::Database;
use gprimitives::H256;
use tokio::sync::{Notify, mpsc};
use tracing::{error, warn};

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
    /// [`crate::MalachiteService::poll_next`].
    pub(crate) event_tx: mpsc::UnboundedSender<Result<MalachiteEvent>>,
    pub(crate) gas_allowance: u64,
    pub(crate) canonical_quarantine: u8,
}

#[async_trait]
impl ethexe_malachite_core::Externalities<SequencerBlock> for EthexeExternalities {
    async fn save_block(
        &self,
        _block_hash: H256,
        block: ethexe_malachite_core::Block<SequencerBlock>,
    ) -> Result<()> {
        let height = block.height;
        let payload = block.payload;
        let mb_keccak = payload.hash();

        // ethexe-malachite-core guarantees ancestor-first save order, so the parent
        // (if any) is already indexed at `height - 1`. For genesis the
        // parent is the pre-genesis sentinel `H256::zero()`.
        let parent_keccak = if height <= 1 {
            H256::zero()
        } else {
            self.db.mb_hash_at_height(height - 1).ok_or_else(|| {
                anyhow!(
                    "save_block: parent at height {} not indexed (height={height}, mb={mb_keccak})",
                    height - 1
                )
            })?
        };

        // Propagate `last_advanced_block` forward — the latest
        // `AdvanceTillEthereumBlock` in this MB wins; otherwise we
        // inherit the parent's value (zero if pre-genesis).
        let parent_advanced = if parent_keccak.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_keccak).last_advanced_block
        };
        let last_advanced = payload
            .transactions
            .iter()
            .rev()
            .find_map(|tx| match tx {
                Transaction::AdvanceTillEthereumBlock { eth_block_hash } => Some(*eth_block_hash),
                _ => None,
            })
            .unwrap_or(parent_advanced);

        self.db.set_mb_block(mb_keccak, payload.clone());
        let parent_for_meta = (!parent_keccak.is_zero()).then_some(parent_keccak);
        self.db.mutate_mb_meta(mb_keccak, |meta| {
            meta.height = height;
            meta.parent_mb_hash = parent_for_meta;
            meta.last_advanced_block = last_advanced;
            // ethexe-malachite-core's ancestor-first ordering means the chain back
            // to genesis is intact by the time `save_block` fires.
            meta.synced = true;
        });
        self.db.set_mb_hash_at_height(height, mb_keccak);

        let _ = self.event_tx.send(Ok(MalachiteEvent::BlockProposal {
            height,
            block: payload,
        }));
        Ok(())
    }

    async fn mark_block_as_finalized(
        &self,
        _block_hash: H256,
        cert: ethexe_malachite_core::CommitCertificate,
    ) -> Result<()> {
        let height = cert.height;
        let mb_keccak = self.db.mb_hash_at_height(height).ok_or_else(|| {
            anyhow!("mark_finalized: no MB indexed at height {height} (save_block must run first)")
        })?;
        let block = self.db.mb_block(mb_keccak).ok_or_else(|| {
            anyhow!("mark_finalized: no SequencerBlock for {mb_keccak} at height {height}")
        })?;

        // Flush the committed injected txs from the mempool and add
        // their hashes to the seen-set so a re-gossip can't slip them
        // back in before they age out.
        let injected: Vec<SignedInjectedTransaction> = block
            .transactions
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
            .globals_mutate(|g| g.latest_finalized_mb_hash = mb_keccak);

        let app_cert = CommitCertificate {
            height,
            block_hash: mb_keccak,
            signatures: cert.signatures,
        };
        let _ = self.event_tx.send(Ok(MalachiteEvent::BlockFinalized {
            cert: app_cert,
            block,
        }));
        Ok(())
    }

    async fn build_block_above(&self, _parent_hash: H256) -> Result<SequencerBlock> {
        // Parent linkage is owned by ethexe-malachite-core (`Block::parent_hash`);
        // we only need the parent's keccak to seed the producer's
        // `last_advanced_block` lookup. `globals.latest_finalized_mb_hash`
        // is advanced by [`Self::mark_block_as_finalized`] before
        // build_block_above runs for the next height, so it's the
        // parent's keccak. For genesis it's `H256::zero()`.
        let parent_keccak = self.db.globals().latest_finalized_mb_hash;
        let parent_advanced = if parent_keccak.is_zero() {
            H256::zero()
        } else {
            self.db.mb_meta(parent_keccak).last_advanced_block
        };

        let (advance, injected) = self.wait_for_proposable_content(parent_advanced).await;

        // Producer pacing — mirrors the old app.rs flow:
        //   1. AdvanceTillEthereumBlock first (if a fresh
        //      quarantine-passed EB exists),
        //   2. then injected user txs,
        //   3. finally the service-level ProgressTasks +
        //      ProcessQueues bookend.
        let mut transactions = Vec::with_capacity(injected.len() + 3);
        if let Some(eth_block_hash) = advance {
            transactions.push(Transaction::AdvanceTillEthereumBlock { eth_block_hash });
        }
        for tx in injected {
            transactions.push(Transaction::Injected(tx));
        }
        transactions.push(Transaction::ProgressTasks {
            limits: ProgressTasksLimits::default(),
        });
        transactions.push(Transaction::ProcessQueues {
            limits: ProcessQueuesLimits::default(),
        });
        Ok(SequencerBlock::new(transactions))
    }

    async fn validate_block_above(&self, block: &ethexe_malachite_core::Block<SequencerBlock>) -> Result<bool> {
        let payload = &block.payload;

        // Parent linkage is enforced by ethexe-malachite-core via `Block::parent_hash`
        // — the consensus layer already rejects proposals that don't
        // extend the locally-finalized chain — so the application has
        // nothing of its own to verify on that axis.

        // (1) At most one AdvanceTillEthereumBlock per MB. Zero is
        // legal (chain still too close to genesis); two+ is a
        // protocol violation.
        let advances: Vec<H256> = payload
            .transactions
            .iter()
            .filter_map(|tx| match tx {
                Transaction::AdvanceTillEthereumBlock { eth_block_hash } => Some(*eth_block_hash),
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

        // (2) Quarantine: the targeted EB must be a canonical
        // ancestor of our local head, deep enough to clear the
        // quarantine window.
        let head = *self.chain_head.read().expect("chain_head poisoned");
        let Some(head) = head else {
            warn!("validate: no chain-head event yet — abstaining from vote");
            return Ok(false);
        };
        let start = self.db.globals().start_block_hash;
        match quarantine::verify_passed(&self.db, head, advance, self.canonical_quarantine, start) {
            Ok(()) => Ok(true),
            Err(e) => {
                warn!(error = %e, advance = %advance, "validate: quarantine reject");
                Ok(false)
            }
        }
    }
}

impl EthexeExternalities {
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
                _ = self.chain_head_notify.notified() => {}
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
            gas_allowance: 1_000_000,
            canonical_quarantine: 0,
        };
        (ext, event_rx)
    }

    /// Build a [`SequencerBlock`] for unit tests.
    ///
    /// The `salt` byte is encoded as the number of leading
    /// `ProgressTasks` placeholders, which gives each block a unique
    /// hash without dragging an extraneous `AdvanceTillEthereumBlock`
    /// through the test (the `last_advanced_block_propagates` case
    /// would otherwise see an unintended advance).
    fn payload(advance: Option<H256>, salt: u8) -> SequencerBlock {
        let mut txs = Vec::with_capacity(salt as usize + 3);
        if let Some(eth) = advance {
            txs.push(Transaction::AdvanceTillEthereumBlock {
                eth_block_hash: eth,
            });
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
        SequencerBlock::new(txs)
    }

    fn wrap(
        payload: SequencerBlock,
        height: u64,
        parent_hash: H256,
    ) -> ethexe_malachite_core::Block<SequencerBlock> {
        ethexe_malachite_core::Block::<SequencerBlock>::new(parent_hash, height, payload)
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
        let mala_hash = block.hash();
        ext.save_block(mala_hash, block).await.unwrap();

        let mb_keccak = p.hash();
        let meta = db.mb_meta(mb_keccak);
        assert_eq!(meta.height, 1);
        assert!(meta.synced);
        assert_eq!(meta.parent_mb_hash, None);
        assert_eq!(db.mb_hash_at_height(1), Some(mb_keccak));
        assert_eq!(db.mb_block(mb_keccak).unwrap(), p);

        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockProposal { height, block } => {
                assert_eq!(height, 1);
                assert_eq!(block, p);
            }
            other => panic!("expected BlockProposal, got {other:?}"),
        }

        // Globals not advanced by save — finalize is what does that.
        assert!(db.globals().latest_finalized_mb_hash.is_zero());
    }

    /// `mark_block_as_finalized` resolves the saved block via
    /// `mb_hash_at_height`, advances `globals.latest_finalized_mb_hash`,
    /// and emits a `BlockFinalized`.
    #[tokio::test]
    async fn finalize_advances_globals_and_emits_event() {
        use ethexe_common::db::GlobalsStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 5);
        ext.save_block(H256::zero(), wrap(p.clone(), 1, H256::zero()))
            .await
            .unwrap();
        let _ = rx.recv().await; // BlockProposal
        ext.mark_block_as_finalized(H256::zero(), fake_cert(1))
            .await
            .unwrap();
        let mb_keccak = p.hash();
        assert_eq!(db.globals().latest_finalized_mb_hash, mb_keccak);
        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockFinalized { cert, block } => {
                assert_eq!(cert.height, 1);
                assert_eq!(cert.block_hash, mb_keccak);
                assert_eq!(block, p);
            }
            other => panic!("expected BlockFinalized, got {other:?}"),
        }
    }

    /// Crash-recovery: build externalities A on a fresh DB, save +
    /// finalize K MBs, drop A, build externalities B on the same DB.
    /// B sees the persisted globals and `mb_hash_at_height` index;
    /// the next `save_block` correctly resolves the parent at
    /// `height-1`. Mirrors what ethexe-malachite-core does after a process restart
    /// once it resumes from `max_finalized_height + 1`.
    #[tokio::test]
    async fn restart_continues_from_persisted_chain() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext_a, mut rx_a) = make_externalities(db.clone());
        let mut payloads = Vec::new();
        for i in 1..=3u64 {
            let p = payload(None, i as u8);
            payloads.push(p.clone());
            ext_a
                .save_block(H256::zero(), wrap(p, i, H256::zero()))
                .await
                .unwrap();
            ext_a
                .mark_block_as_finalized(H256::zero(), fake_cert(i))
                .await
                .unwrap();
        }
        // Drain events so the channel doesn't hold stale references.
        while rx_a.try_recv().is_ok() {}
        drop(ext_a);
        drop(rx_a);

        // After "restart" — fresh externalities on the SAME DB.
        let (ext_b, mut rx_b) = make_externalities(db.clone());

        // Pre-restart pointers must survive.
        assert_eq!(db.globals().latest_finalized_mb_hash, payloads[2].hash());
        for (i, p) in payloads.iter().enumerate() {
            let h = (i + 1) as u64;
            assert_eq!(db.mb_hash_at_height(h), Some(p.hash()));
            assert_eq!(
                db.mb_meta(p.hash()).parent_mb_hash,
                if i == 0 {
                    None
                } else {
                    Some(payloads[i - 1].hash())
                }
            );
        }

        // Save + finalize MB at height 4 — the parent resolution
        // must see the height-3 record left by the previous run.
        let p4 = payload(None, 99);
        ext_b
            .save_block(H256::zero(), wrap(p4.clone(), 4, H256::zero()))
            .await
            .unwrap();
        let _ = rx_b.recv().await; // proposal
        ext_b
            .mark_block_as_finalized(H256::zero(), fake_cert(4))
            .await
            .unwrap();
        assert_eq!(
            db.mb_meta(p4.hash()).parent_mb_hash,
            Some(payloads[2].hash())
        );
        assert_eq!(db.globals().latest_finalized_mb_hash, p4.hash());
    }

    /// `last_advanced_block` is propagated forward: an MB without an
    /// `AdvanceTillEthereumBlock` inherits the parent's value; an MB
    /// with one resets it.
    #[tokio::test]
    async fn last_advanced_block_propagates() {
        use ethexe_common::db::MbStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());

        let h1 = payload(None, 1);
        let h2 = payload(Some(H256::repeat_byte(0xAB)), 2);
        let h3 = payload(None, 3);

        for (i, p) in [&h1, &h2, &h3].iter().enumerate() {
            let height = (i + 1) as u64;
            ext.save_block(H256::zero(), wrap((*p).clone(), height, H256::zero()))
                .await
                .unwrap();
            ext.mark_block_as_finalized(H256::zero(), fake_cert(height))
                .await
                .unwrap();
        }
        while rx.try_recv().is_ok() {}

        assert!(db.mb_meta(h1.hash()).last_advanced_block.is_zero());
        assert_eq!(
            db.mb_meta(h2.hash()).last_advanced_block,
            H256::repeat_byte(0xAB),
            "h2 should anchor to its own AdvanceTillEthereumBlock"
        );
        assert_eq!(
            db.mb_meta(h3.hash()).last_advanced_block,
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
        let block = ethexe_malachite_core::Block::<SequencerBlock>::new(
            H256::zero(),
            1,
            SequencerBlock::new(vec![
                Transaction::AdvanceTillEthereumBlock {
                    eth_block_hash: H256::repeat_byte(0xAA),
                },
                Transaction::AdvanceTillEthereumBlock {
                    eth_block_hash: H256::repeat_byte(0xBB),
                },
            ]),
        );
        assert!(!ext.validate_block_above(&block).await.unwrap());
    }

    #[tokio::test]
    async fn validate_abstains_without_chain_head() {
        // One AdvanceTillEthereumBlock + no observer head yet — the
        // application can't verify the candidate's quarantine status,
        // so the vote is `Ok(false)` rather than `Err`.
        let db = Database::memory();
        // The `start_block` fence + missing chain-head trigger the
        // abstain path.
        let (ext, _rx) = make_externalities(db.clone());
        let block = ethexe_malachite_core::Block::<SequencerBlock>::new(
            H256::zero(),
            1,
            SequencerBlock::new(vec![Transaction::AdvanceTillEthereumBlock {
                eth_block_hash: H256::repeat_byte(0xCC),
            }]),
        );
        assert!(!ext.validate_block_above(&block).await.unwrap());
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

        let block = ethexe_malachite_core::Block::<SequencerBlock>::new(
            H256::zero(),
            1,
            SequencerBlock::new(vec![Transaction::AdvanceTillEthereumBlock {
                eth_block_hash: chain_hashes[1].0,
            }]),
        );
        assert!(ext.validate_block_above(&block).await.unwrap());
    }
}
