// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`MalachiteService`] — public façade.
//!
//! Wraps [`ethexe_malachite_core::MalachiteService`] with the ethexe-shaped API the
//! rest of the workspace already consumes. Owns:
//!
//! - the chain-head register that [`Self::receive_new_chain_head`]
//!   updates and [`crate::EthexeExternalities`] reads,
//! - the [`Mempool`] handle that serves both injected-tx routing and
//!   the producer's content selection,
//! - the inner [`ethexe_malachite_core::MalachiteService`] itself, polled inline so
//!   any `Err` item surfaces on this service's stream and so
//!   [`Self::shutdown`] can `await` the engine actor's full teardown
//!   (releasing the RocksDB advisory lock before
//!   re-opening on the same home directory).

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, anyhow};
use ethexe_common::{
    Address, SimpleBlockData,
    db::{ConfigStorageRO, OnChainStorageRO},
    injected::SignedInjectedTransaction,
};
use ethexe_db::Database;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use gsigner::{Signer, schemes::secp256k1::Secp256k1};
use tokio::sync::{Notify, mpsc};

use crate::{
    MalachiteConfig, MalachiteEvent, Mempool, ValidatorEntry, externalities::EthexeExternalities,
};

/// Public consensus service.
pub struct MalachiteService {
    events_rx: mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    chain_head: Arc<RwLock<Option<SimpleBlockData>>>,
    chain_head_notify: Arc<Notify>,
    mempool: Arc<dyn Mempool>,
    /// Shared with the inner engine — held here so
    /// [`Self::receive_new_chain_head`] can release pending events
    /// whose `last_advanced_eb` Eth block has just been synced
    /// by the observer.
    externalities: Arc<EthexeExternalities>,
    /// On-chain validator addresses only — we keep operator-supplied
    /// pub keys here so era rotations can resolve them back.
    validator_pool: HashMap<Address, gsigner::schemes::secp256k1::PublicKey>,
    /// Era of the set currently in the engine; gates rotation no-ops.
    active_era: Option<u64>,
    /// Inner ethexe-malachite-core service. Held in an `Option` so
    /// [`Self::shutdown`] can `take` it and `await` its
    /// async-shutdown method without violating the `Drop` signature.
    inner:
        Option<ethexe_malachite_core::MalachiteService<crate::Transactions, EthexeExternalities>>,
}

impl Drop for MalachiteService {
    fn drop(&mut self) {
        // Best-effort cleanup if the caller didn't go through
        // [`Self::shutdown`]: the inner ethexe-malachite-core service runs its own
        // kill/abort sequence inside its `Drop` impl. RocksDB locks
        // and listening sockets release asynchronously after that,
        // so a sync drop alone is unsafe to immediately re-open the
        // same home directory. Use `shutdown().await` instead when
        // an immediate restart is required.
        let _ = self.inner.take();
    }
}

impl MalachiteService {
    /// Bootstrap the consensus service.
    ///
    /// Parameters:
    /// - `signer` — shared ethexe key manager; the secret matching
    ///   `validator_pub_key` is extracted once here and passed into
    ///   ethexe-malachite-core as the validator secret.
    /// - `validator_pub_key` — this node's validator public key. When
    ///   `Some`, it must appear in [`MalachiteConfig::validators`] and
    ///   the engine starts in [`ethexe_malachite_core::NodeRole::Validator`].
    ///   When `None`, the engine starts as a full / connect node
    ///   ([`ethexe_malachite_core::NodeRole::FullNode`]): joins gossip
    ///   and sync but never signs anything. A fresh ephemeral key
    ///   provides the libp2p peer identity in that case.
    /// - `db` — shared ethexe [`Database`] used by the externalities
    ///   to persist MBs and walk parent links.
    /// - `mempool` — source of injected user transactions for the
    ///   producer; also the sink for [`Self::receive_injected_transaction`].
    pub async fn new(
        config: MalachiteConfig,
        db: Database,
        signer: Signer<Secp256k1>,
        validator_pub_key: Option<gsigner::schemes::secp256k1::PublicKey>,
        mempool: Arc<dyn Mempool>,
    ) -> Result<Self> {
        tracing::info!(
            listen = %config.listen_addr,
            persistent_peers = config.persistent_peers.len(),
            validators = config.validators.len(),
            role = if validator_pub_key.is_some() { "validator" } else { "full" },
            "Bootstrapping Malachite engine",
        );

        std::fs::create_dir_all(&config.home_dir)
            .with_context(|| format!("creating Malachite home dir {:?}", config.home_dir))?;

        if config.validators.is_empty() {
            return Err(anyhow!("MalachiteConfig::validators is empty"));
        }

        // Validators sign votes/proposals using their on-chain key;
        // full nodes get an ephemeral secret used only as the libp2p
        // peer identity.
        let (role, validator_secret) = match validator_pub_key {
            Some(pub_key) => {
                if !config.validators.iter().any(|v| v.public_key == pub_key) {
                    return Err(anyhow!(
                        "local validator {pub_key} not present in MalachiteConfig::validators"
                    ));
                }
                let secret = signer
                    .private_key(pub_key)
                    .context("extracting validator private key from signer")?;
                (ethexe_malachite_core::NodeRole::Validator, secret)
            }
            None => (
                ethexe_malachite_core::NodeRole::FullNode,
                gsigner::schemes::secp256k1::PrivateKey::random(),
            ),
        };

        // Build the ethexe-malachite-core-side config. Application-side knobs
        // (gas allowance, quarantine depth) stay in [`MalachiteConfig`]
        // and travel into the externalities; they never reach
        // ethexe-malachite-core.
        let svc_cfg = ethexe_malachite_core::MalachiteConfig {
            listen_addr: config.listen_addr,
            base: config.home_dir.clone(),
            persistent_peers: config.persistent_peers.clone(),
            validator_secret,
            validators: config.validators.clone(),
            role,
            // Producer waits up to one Ethereum slot for a fresh EB past quarantine.
            propose_timeout: alloy::eips::merge::SLOT_DURATION,
        };

        let chain_head = Arc::new(RwLock::new(None));
        let chain_head_notify = Arc::new(Notify::new());
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        let externalities = Arc::new(EthexeExternalities {
            db,
            mempool: Arc::clone(&mempool),
            chain_head: Arc::clone(&chain_head),
            chain_head_notify: Arc::clone(&chain_head_notify),
            event_tx: events_tx,
            pending_events: std::sync::Mutex::new(std::collections::VecDeque::new()),
            gas_allowance: config.gas_allowance,
            canonical_quarantine: config.canonical_quarantine,
            post_quarantine_delay: config.post_quarantine_delay,
        });

        // On-chain addresses → pub keys, so era rotations resolve back without an out-of-band lookup.
        let validator_pool: HashMap<Address, gsigner::schemes::secp256k1::PublicKey> = config
            .validators
            .iter()
            .map(|v| (v.public_key.to_address(), v.public_key))
            .collect();

        let inner =
            ethexe_malachite_core::MalachiteService::new(svc_cfg, Arc::clone(&externalities))
                .await
                .map_err(|e| anyhow!("starting ethexe-malachite-core: {e}"))?;

        Ok(Self {
            events_rx,
            chain_head,
            chain_head_notify,
            mempool,
            externalities,
            validator_pool,
            active_era: None,
            inner: Some(inner),
        })
    }

    /// Hand an injected transaction to the mempool. The local
    /// producer pulls from the same pool when assembling the next MB.
    pub fn receive_injected_transaction(
        &self,
        tx: SignedInjectedTransaction,
    ) -> Result<(), crate::mempool::MempoolInsertError> {
        self.mempool.insert(tx)
    }

    /// Feed an observer-delivered Ethereum `BlockSynced` block into the
    /// service. `BlockSynced` events arrive out-of-order, so this method
    /// guarantees two invariants the producer relies on:
    ///
    /// 1. The chain-head register is monotone in **height** — a stale
    ///    older head would push `anchor = head - quarantine` below
    ///    `parent_advanced` and stall the producer for `propose_timeout`.
    /// 2. Every `BlockSynced` fires `chain_head_notify`, even when height
    ///    didn't move. A lower-height sync may have just landed parent
    ///    headers the producer's `is_strict_descendant_of` walk needs;
    ///    without this kick a failed walk would never retry.
    ///
    /// Also drains any queued [`MalachiteEvent::BlockProposal`] /
    /// [`MalachiteEvent::BlockFinalized`] whose `last_advanced_eb`
    /// Eth block has now landed in the local DB — keeps the strict
    /// FIFO ordering compute and the malachite engine rely on.
    pub fn receive_new_chain_head(&mut self, head: SimpleBlockData) {
        // Rotate before waking the producer so the next round sees the new set.
        self.maybe_rotate_validators_for_era(&head);

        let mut current = self.chain_head.write().expect("chain_head poisoned");
        let advanced = match current.as_ref() {
            Some(existing) => head.header.height > existing.header.height,
            None => true,
        };
        if advanced {
            *current = Some(head);
        }
        drop(current);
        // Wake the producer regardless of whether height moved — see
        // invariant #2 in the doc above.
        self.chain_head_notify.notify_one();
        if advanced {
            // let eb_hash = head.hash;
            let purged_txs = self.mempool.set_chain_head(head);
            if !purged_txs.is_empty() {
                let event = MalachiteEvent::PurgedTransactions {
                    eb_hash: head.hash,
                    transactions: purged_txs,
                };
                if let Err(err) = self.externalities.event_tx.send(Ok(event)) {
                    tracing::error!(
                        event = ?err.0,
                        "malachite-service: event_tx channel closed, failed to send purged transactions event"
                    );
                };
            }
        }
        self.externalities.drain_pending_events();
    }

    /// Forward a `ComputeEvent::BlockPrepared` notification so any
    /// pending [`MalachiteEvent`] whose `last_advanced_eb` was the
    /// freshly-prepared block can be released. Prepared blocks are
    /// the prerequisite for downstream `compute_mb` not racing the
    /// code-validation pipeline — see the prerequisite check inside
    /// the externalities impl.
    pub fn receive_eb_prepared(&self, _eb_hash: H256) {
        // Drain inspects each queued entry's prerequisite against the
        // current `block_meta.prepared` flag, so we don't need to use
        // `_eb_hash` here — the FIFO drain releases everything that
        // newly satisfies its prerequisite.
        self.externalities.drain_pending_events();
    }

    /// Push the on-chain validators for `head`'s era into the engine,
    /// if the era moved. Skips on missing DB data or unknown pub keys
    /// (wait-and-retry: the next `BlockSynced` re-evaluates).
    fn maybe_rotate_validators_for_era(&mut self, head: &SimpleBlockData) {
        let db = &self.externalities.db;
        let timelines = db.config().timelines;
        let Some(era) = timelines.era_from_ts(head.header.timestamp) else {
            return;
        };
        if self.active_era == Some(era) {
            return;
        }
        let Some(addrs) = db.validators(era) else {
            tracing::trace!(era, "validators for era not yet in DB; deferring rotation");
            return;
        };

        let mut new_set = Vec::with_capacity(self.validator_pool.len());
        let mut missing: Vec<Address> = Vec::new();
        for addr in addrs.iter() {
            match self.validator_pool.get(addr) {
                Some(pk) => new_set.push(ValidatorEntry {
                    public_key: *pk,
                    voting_power: 1,
                }),
                None => missing.push(*addr),
            }
        }

        if !missing.is_empty() {
            tracing::warn!(
                era,
                missing = ?missing,
                "validator pool missing pub keys for some on-chain era validators; \
                 keeping the previous active set",
            );
            return;
        }

        // Bug-class failure — advance active_era so we don't loop on the same broken input.
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                tracing::error!(era, "rotate after shutdown");
                self.active_era = Some(era);
                return;
            }
        };
        if let Err(e) = inner.update_validators(new_set) {
            tracing::error!(era, error = %e, "rotating malachite validator set failed");
            self.active_era = Some(era);
            return;
        }
        self.active_era = Some(era);
        tracing::info!(
            era,
            "rotated malachite validator set to era's on-chain quorum"
        );
    }

    /// Shut the inner ethexe-malachite-core service down deterministically.
    ///
    /// Unlike `Drop` (which is fire-and-forget), this future awaits
    /// the engine actor's tear-down, releasing the WAL / RocksDB
    /// advisory lock and the libp2p listener socket BEFORE
    /// returning. Tests that immediately re-open the same home
    /// directory (or the same `Database` for that matter) need this;
    /// production node shutdown is also better off going through
    /// here so cleanup races don't leak into the next start.
    pub async fn shutdown(mut self) {
        if let Some(inner) = self.inner.take() {
            inner.shutdown().await;
        }
    }
}

impl Stream for MalachiteService {
    type Item = Result<MalachiteEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // The inner stream is errors-only (since ethexe-malachite-core
        // no longer emits events — events flow exclusively through
        // EthexeExternalities into our own `events_rx`). Forward each
        // inner error onto our outer stream.
        if let Some(inner) = self.inner.as_mut() {
            match Pin::new(&mut *inner).poll_next(cx) {
                Poll::Ready(Some(e)) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    self.inner = None;
                }
                Poll::Pending => {}
            }
        }
        self.events_rx.poll_recv(cx)
    }
}

impl FusedStream for MalachiteService {
    fn is_terminated(&self) -> bool {
        self.events_rx.is_closed()
    }
}
