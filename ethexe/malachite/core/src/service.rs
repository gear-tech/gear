// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`MalachiteCore`] — the public entry point.

use crate::{
    app,
    codec::ScaleCodec,
    config::{MalachiteCoreConfig, NodeRole},
    context::{MalachiteCtx, Validator, ValidatorSet},
    externalities::Externalities,
    signing::{MalachiteSigner, private_key_from_gsigner, public_key_from_gsigner},
    state::{SharedValidatorSet, State},
    store::Store,
    types::Address,
};
use advisory_lock::{AdvisoryFileLock, FileLockMode};
use anyhow::{Context as _, Result};
use futures::{Stream, stream::FusedStream};
use malachitebft_app_channel::{
    ConsensusContext, EngineBuilder, EngineHandle, NetworkMsg, RequestContext, SyncContext,
    WalContext,
    app::{
        config::{
            ConsensusConfig, LoggingConfig, MetricsConfig, NodeConfig, P2pConfig, RuntimeConfig,
            ValuePayload, ValueSyncConfig,
        },
        metrics::SharedRegistry,
    },
};
use malachitebft_engine::network::NetworkRef;
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

pub type MalachiteNetworkParts = (
    NetworkRef<MalachiteCtx>,
    mpsc::Sender<NetworkMsg<MalachiteCtx>>,
);

/// Trait-object-friendly facade for the service. The stream carries
/// only fatal app-task errors — successful events reach the
/// application through [`Externalities`] callbacks instead.
pub trait MService: Stream<Item = anyhow::Error> + Send + Unpin {}

/// Application-agnostic Malachite BFT consensus service.
pub struct MalachiteCore<EXT: Externalities> {
    /// Fatal errors forwarded from the app task.
    errors_rx: mpsc::UnboundedReceiver<anyhow::Error>,
    /// Handle to the malachite engine actor tree.
    engine: EngineHandle,
    /// Handle to the spawned app event-loop task.
    app_handle: JoinHandle<()>,
    /// WAL file path; [`Self::shutdown`] probes its advisory lock before
    /// returning so a restart on the same base dir doesn't race the writer.
    wal_path: PathBuf,
    /// Shared with the app loop; [`Self::update_validators`] writes here.
    validator_set: SharedValidatorSet,
    /// Keeps the externalities alive for the app task.
    _externalities: Arc<EXT>,
}

/// Upper bound on how long [`MalachiteCore::shutdown`] waits for the WAL
/// advisory lock to release after the engine actor has stopped.
const WAL_LOCK_RELEASE_TIMEOUT: Duration = Duration::from_secs(10);
const WAL_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Block until the WAL file is no longer locked or `timeout` elapses.
/// The WAL writer is a detached thread, so the engine actor's JoinHandle
/// is not a sufficient barrier; probing the lock lets the caller re-open
/// the same base dir right away. A missing WAL file passes immediately.
async fn wait_wal_lock_released(wal_path: &Path, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match OpenOptions::new().read(true).write(true).open(wal_path) {
            Ok(file) => match AdvisoryFileLock::try_lock(&file, FileLockMode::Exclusive) {
                Ok(()) => return,
                Err(_) => {
                    if tokio::time::Instant::now() >= deadline {
                        tracing::warn!(
                            target: "ethexe-malachite-core",
                            wal = %wal_path.display(),
                            "WAL advisory lock did not release within {:?}; \
                             the next service start on the same base dir may fail",
                            timeout,
                        );
                        return;
                    }
                    tokio::time::sleep(WAL_LOCK_POLL_INTERVAL).await;
                }
            },
            // No WAL on disk → nothing to wait for. Any other I/O
            // error (permissions, etc.) is not a lock issue — surface
            // it lazily on the next `new()` instead of blocking here.
            Err(_) => return,
        }
    }
}

impl<EXT: Externalities> Drop for MalachiteCore<EXT> {
    fn drop(&mut self) {
        // Fire-and-forget shutdown; locks and sockets may take a moment to
        // release. Use [`Self::shutdown`] when re-opening the same base dir.
        self.engine.actor.kill();
        self.app_handle.abort();
        self.engine.handle.abort();
    }
}

impl<EXT: Externalities> MalachiteCore<EXT> {
    /// Block until the engine actor tree has shut down and the file locks
    /// (RocksDB, WAL) are released — required before re-opening the same `base`.
    pub async fn shutdown(mut self) {
        self.engine.actor.kill();
        // `kill` is asynchronous — await the JoinHandles.
        let _ = (&mut self.engine.handle).await;
        self.app_handle.abort();
        let _ = (&mut self.app_handle).await;
        // The WAL writer is a detached thread; probe its lock explicitly.
        wait_wal_lock_released(&self.wal_path, WAL_LOCK_RELEASE_TIMEOUT).await;
    }
}

impl<EXT: Externalities> MalachiteCore<EXT> {
    /// Bootstrap the service.
    pub async fn new(
        config: MalachiteCoreConfig,
        externalities: Arc<EXT>,
        (network_ref, tx_network): MalachiteNetworkParts,
    ) -> Result<Self> {
        // The service owns `<base>/malachite/`. We `mkdir -p` it so
        // RocksDB and the WAL can land there.
        let svc_dir = config.base.join("malachite");
        std::fs::create_dir_all(&svc_dir)
            .with_context(|| format!("creating service dir {:?}", svc_dir))?;
        let wal_path = svc_dir.join("consensus.wal");
        let store_path = svc_dir.join("store.db");

        // ---- key + libp2p identity ----
        let private_key = private_key_from_gsigner(&config.validator_secret)
            .context("converting validator secret")?;
        let signer = MalachiteSigner::new(private_key);
        let public_key = signer.public_key();
        let address = Address::from_public_key(&public_key);
        let moniker = format!("v-{}", &address.to_string()[..10]);

        tracing::info!(
            target: "ethexe-malachite-core",
            %moniker,
            address = %address,
            validators = config.validators.len(),
            role = ?config.role,
            "Bootstrapping Malachite engine",
        );

        // ---- validator set from config ----
        if config.validators.is_empty() {
            return Err(anyhow::anyhow!("MalachiteCoreConfig::validators is empty"));
        }
        let mut validators = Vec::with_capacity(config.validators.len());
        for entry in &config.validators {
            let pk = public_key_from_gsigner(&entry.public_key)
                .context("converting validator public key")?;
            validators.push(Validator::new(pk, entry.voting_power));
        }
        let initial_validator_set = ValidatorSet::new(validators);
        let in_set = initial_validator_set.get_by_address(&address).is_some();
        let validator_set = SharedValidatorSet::new(initial_validator_set);

        match config.role {
            NodeRole::Validator if !in_set => {
                return Err(anyhow::anyhow!(
                    "NodeRole::Validator: local address {address} not present in MalachiteCoreConfig::validators"
                ));
            }
            NodeRole::FullNode if in_set => {
                return Err(anyhow::anyhow!(
                    "NodeRole::FullNode: local address {address} must NOT be in MalachiteCoreConfig::validators"
                ));
            }
            NodeRole::Validator | NodeRole::FullNode => {}
        }

        // ---- engine ----
        let inner_cfg = build_inner_config(&moniker);
        let ctx = MalachiteCtx::new();
        let consensus_signer = MalachiteSigner::new(signer.private_key().clone());
        let (channels, engine) = EngineBuilder::new(ctx.clone(), inner_cfg)
            .with_default_wal(WalContext::new(wal_path.clone(), ScaleCodec))
            .with_custom_network(network_ref, tx_network)
            .with_default_consensus(ConsensusContext::new(address, consensus_signer))
            .with_default_sync(SyncContext::new(ScaleCodec))
            .with_default_request(RequestContext::new(100))
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("building Malachite engine: {e}"))?;

        // Side-effect: register metrics moniker so the prometheus
        // namespace is unique per node.
        let _registry = SharedRegistry::global().with_moniker(&moniker);

        // ---- store + state ----
        let store = Store::open(&store_path).context("opening Store")?;
        let state = State::new(
            signer,
            validator_set.clone(),
            address,
            store,
            config.propose_timeout,
        )?;

        // ---- spawn app task ----
        let (errors_tx, errors_rx) = mpsc::unbounded_channel();
        let externalities_for_task = Arc::clone(&externalities);
        let app_handle = tokio::spawn(async move {
            if let Err(e) = app::run::<EXT>(state, channels, externalities_for_task).await {
                tracing::error!(target: "ethexe-malachite-core", error = %e, "app task terminated");
                let _ = errors_tx.send(e);
            }
        });

        Ok(Self {
            errors_rx,
            engine,
            app_handle,
            wal_path,
            validator_set,
            _externalities: externalities,
        })
    }

    /// Swap the active validator set, taking effect at the next height start
    /// (the current height runs to completion with the old set).
    /// The caller must keep the local key in the set while in
    /// [`NodeRole::Validator`]. Empty input is rejected.
    pub fn update_validators(&self, validators: Vec<crate::ValidatorEntry>) -> Result<()> {
        if validators.is_empty() {
            return Err(anyhow::anyhow!(
                "MalachiteCore::update_validators: empty validators list"
            ));
        }
        let mut converted = Vec::with_capacity(validators.len());
        for entry in &validators {
            let pk = public_key_from_gsigner(&entry.public_key)
                .context("converting validator public key")?;
            converted.push(Validator::new(pk, entry.voting_power));
        }
        let new_set = ValidatorSet::new(converted);
        self.validator_set.update(new_set);
        Ok(())
    }
}

impl<EXT: Externalities> Stream for MalachiteCore<EXT> {
    type Item = anyhow::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.errors_rx.poll_recv(cx)
    }
}

impl<EXT: Externalities> FusedStream for MalachiteCore<EXT> {
    fn is_terminated(&self) -> bool {
        self.errors_rx.is_closed()
    }
}

impl<EXT: Externalities> MService for MalachiteCore<EXT> {}

fn build_inner_config(moniker: &str) -> InnerNodeConfig {
    let consensus = ConsensusConfig {
        enabled: true,
        value_payload: ValuePayload::ProposalAndParts,
        queue_capacity: 100,
        // NOTE: the config is actually unused because we have our own network implementation
        p2p: P2pConfig::default(),
    };
    InnerNodeConfig {
        moniker: moniker.to_string(),
        consensus,
        value_sync: ValueSyncConfig::default(),
        logging: LoggingConfig::default(),
        metrics: MetricsConfig::default(),
        runtime: RuntimeConfig::default(),
    }
}

#[derive(Clone, Debug)]
struct InnerNodeConfig {
    moniker: String,
    consensus: ConsensusConfig,
    value_sync: ValueSyncConfig,
    #[allow(dead_code)]
    logging: LoggingConfig,
    #[allow(dead_code)]
    metrics: MetricsConfig,
    #[allow(dead_code)]
    runtime: RuntimeConfig,
}

impl NodeConfig for InnerNodeConfig {
    fn moniker(&self) -> &str {
        &self.moniker
    }

    fn consensus(&self) -> &ConsensusConfig {
        &self.consensus
    }

    fn consensus_mut(&mut self) -> &mut ConsensusConfig {
        &mut self.consensus
    }

    fn value_sync(&self) -> &ValueSyncConfig {
        &self.value_sync
    }

    fn value_sync_mut(&mut self) -> &mut ValueSyncConfig {
        &mut self.value_sync
    }
}
