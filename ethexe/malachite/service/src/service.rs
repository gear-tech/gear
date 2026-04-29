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
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, anyhow};
use ethexe_common::{SimpleBlockData, injected::SignedInjectedTransaction};
use ethexe_db::Database;
use futures::{Stream, stream::FusedStream};
use gsigner::{Signer, schemes::secp256k1::Secp256k1};
use tokio::sync::{Notify, mpsc};

use crate::{MalachiteConfig, MalachiteEvent, Mempool, externalities::EthexeExternalities};

/// Public consensus service.
pub struct MalachiteService {
    events_rx: mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    chain_head: Arc<RwLock<Option<SimpleBlockData>>>,
    chain_head_notify: Arc<Notify>,
    mempool: Arc<dyn Mempool>,
    /// Inner ethexe-malachite-core service. Held in an `Option` so
    /// [`Self::shutdown`] can `take` it and `await` its
    /// async-shutdown method without violating the `Drop` signature.
    inner:
        Option<ethexe_malachite_core::MalachiteService<crate::SequencerBlock, EthexeExternalities>>,
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
    /// - `validator_pub_key` — this node's validator public key; must
    ///   appear in [`MalachiteConfig::validators`].
    /// - `db` — shared ethexe [`Database`] used by the externalities
    ///   to persist MBs and walk parent links.
    /// - `mempool` — source of injected user transactions for the
    ///   producer; also the sink for [`Self::receive_injected_transaction`].
    pub async fn new(
        config: MalachiteConfig,
        db: Database,
        signer: Signer<Secp256k1>,
        validator_pub_key: gsigner::schemes::secp256k1::PublicKey,
        mempool: Arc<dyn Mempool>,
    ) -> Result<Self> {
        tracing::info!(
            listen = %config.listen_addr,
            persistent_peers = config.persistent_peers.len(),
            validators = config.validators.len(),
            "Bootstrapping Malachite engine",
        );

        std::fs::create_dir_all(&config.home_dir)
            .with_context(|| format!("creating Malachite home dir {:?}", config.home_dir))?;

        // Sanity: the local validator must appear in the configured
        // set, otherwise ethexe-malachite-core will reject the start-up anyway.
        // Catching it here gives a clearer error.
        if config.validators.is_empty() {
            return Err(anyhow!("MalachiteConfig::validators is empty"));
        }
        if !config
            .validators
            .iter()
            .any(|v| v.public_key == validator_pub_key)
        {
            return Err(anyhow!(
                "local validator {validator_pub_key} not present in MalachiteConfig::validators"
            ));
        }

        let validator_secret = signer
            .private_key(validator_pub_key)
            .context("extracting validator private key from signer")?;

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
            role: ethexe_malachite_core::NodeRole::Validator,
            // Producer waits up to one Ethereum slot for a fresh EB
            // past quarantine. Matches the old NON_PROPOSER_PROPOSE
            // window the previous app.rs configured.
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
            gas_allowance: config.gas_allowance,
            canonical_quarantine: config.canonical_quarantine,
        });

        let inner = ethexe_malachite_core::MalachiteService::new(svc_cfg, externalities)
            .await
            .map_err(|e| anyhow!("starting ethexe-malachite-core: {e}"))?;

        Ok(Self {
            events_rx,
            chain_head,
            chain_head_notify,
            mempool,
            inner: Some(inner),
        })
    }

    /// Hand an injected transaction to the mempool. The local
    /// producer pulls from the same pool when assembling the next MB.
    pub fn receive_injected_transaction(&self, tx: SignedInjectedTransaction) {
        self.mempool.insert(tx);
    }

    /// Feed the latest observer-delivered Ethereum chain head into
    /// the service. Updates both the producer's view (used by
    /// [`ethexe_malachite_core::Externalities::build_block_above`]) and the
    /// mempool's GC head.
    pub fn receive_new_chain_head(&mut self, head: SimpleBlockData) {
        *self.chain_head.write().expect("chain_head poisoned") = Some(head);
        // Wake the producer if it was idling on `wait_for_new_tx` /
        // `wait_for_chain_head` — see
        // [`crate::EthexeExternalities::wait_for_proposable_content`].
        self.chain_head_notify.notify_waiters();
        self.mempool.set_chain_head(head);
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
        // Drain any pending Err from the inner stream so engine-side
        // failures surface here. The inner Ok items are intentionally
        // dropped — our visible events are emitted exclusively from
        // the externalities into `events_rx`.
        if let Some(inner) = self.inner.as_mut() {
            loop {
                match Pin::new(&mut *inner).poll_next(cx) {
                    Poll::Ready(Some(Ok(_))) => continue,
                    Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                    Poll::Ready(None) => {
                        self.inner = None;
                        break;
                    }
                    Poll::Pending => break,
                }
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
