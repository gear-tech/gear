// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`MalachiteService`] — the public entry point.

use crate::{
    app,
    codec::ScaleCodec,
    config::{Environment, MalachiteConfig, NodeRole},
    context::{MalachiteCtx, Validator, ValidatorSet},
    externalities::{BlockPayload, Externalities},
    signing::{
        MalachiteSigner, libp2p_keypair_from, private_key_from_gsigner, public_key_from_gsigner,
    },
    state::{SharedValidatorSet, State},
    store::Store,
    types::Address,
};
use advisory_lock::{AdvisoryFileLock, FileLockMode};
use anyhow::{Context as _, Result};
use bytes::Bytes;
use futures::{Stream, stream::FusedStream};
use malachitebft_app_channel::{
    ConsensusContext, EngineBuilder, EngineHandle, NetworkContext, NetworkIdentity, RequestContext,
    SigningProviderExt, SyncContext, WalContext,
    app::{
        config::{
            ConsensusConfig, DiscoveryConfig, LoggingConfig, MetricsConfig, NodeConfig, P2pConfig,
            PubSubProtocol, RuntimeConfig, TransportProtocol, ValuePayload, ValueSyncConfig,
        },
        metrics::SharedRegistry,
    },
};
use malachitebft_core_types::ValidatorProof;
use std::{
    fs::OpenOptions,
    marker::PhantomData,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

/// Trait-object-friendly facade for the service. The stream carries
/// only fatal app-task errors — successful events reach the
/// application through [`Externalities`] callbacks instead.
pub trait MService: Stream<Item = anyhow::Error> + Send + Unpin {}

/// Application-agnostic Malachite BFT consensus service.
pub struct MalachiteService<P: BlockPayload, EXT: Externalities<P>> {
    errors_rx: mpsc::UnboundedReceiver<anyhow::Error>,
    engine: EngineHandle,
    app_handle: JoinHandle<()>,
    /// Path to the WAL file. [`Self::shutdown`] probes the advisory
    /// lock on this path before returning so the next service
    /// opening the same base dir does not race the WAL writer thread.
    wal_path: PathBuf,
    /// Shared with the inner app loop; [`Self::update_validators`]
    /// writes here, the next `Finalized` / `ConsensusReady` reply reads.
    validator_set: SharedValidatorSet,
    _externalities: Arc<EXT>,
    _phantom: PhantomData<fn() -> P>,
}

/// Upper bound on how long [`MalachiteService::shutdown`] will wait
/// for the WAL advisory lock to be released after the engine actor
/// has stopped. Empirically the writer thread drops the file within
/// tens of milliseconds; this ceiling guards against pathological CI
/// scheduling without ever blocking healthy shutdowns.
const WAL_LOCK_RELEASE_TIMEOUT: Duration = Duration::from_secs(10);
const WAL_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Block until the WAL file at `wal_path` is no longer locked, or
/// `timeout` has elapsed.
///
/// Malachite's WAL is owned by a [`std::thread`] (see
/// `arc-malachitebft-engine`'s `wal::thread`); the engine actor's
/// `post_stop` only sends a `Shutdown` message on a channel, and the
/// thread releases its [`advisory_lock`] when it later drops the log.
/// The actor's `JoinHandle` is therefore *not* a sufficient barrier —
/// the writer thread can still be live (and the lock still held) after
/// the engine task exits. We probe the lock here so callers of
/// [`MalachiteService::shutdown`] can immediately re-open the same
/// base dir without spurious "advisory lock held" errors.
///
/// A missing WAL file means we never wrote one; nothing to wait for.
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

impl<P: BlockPayload, EXT: Externalities<P>> Drop for MalachiteService<P, EXT> {
    fn drop(&mut self) {
        // Stop the engine actor so its libp2p / consensus children
        // shut down cleanly, then abort the app and engine join handles.
        // Note: this is a fire-and-forget shutdown — RocksDB locks
        // and listening sockets may take a few hundred ms to release.
        // Use [`Self::shutdown`] for tests that immediately re-open
        // the same home directory.
        self.engine.actor.kill();
        self.app_handle.abort();
        self.engine.handle.abort();
    }
}

impl<P: BlockPayload, EXT: Externalities<P>> MalachiteService<P, EXT> {
    /// Block until the engine actor tree has finished shutting down
    /// and any open file locks (RocksDB, WAL) have been released.
    /// Use this before re-opening the same `base` to avoid
    /// "advisory lock held" errors at the second `new()` call.
    pub async fn shutdown(mut self) {
        self.engine.actor.kill();
        // `kill` is asynchronous — the actor finishes its current
        // message and then stops, so we await the JoinHandles.
        let _ = (&mut self.engine.handle).await;
        self.app_handle.abort();
        let _ = (&mut self.app_handle).await;
        // The engine task exiting doesn't synchronously release the
        // WAL advisory lock — the writer is a detached std::thread.
        // Probe the lock so callers can immediately re-open the same
        // base dir.
        wait_wal_lock_released(&self.wal_path, WAL_LOCK_RELEASE_TIMEOUT).await;
    }
}

impl<P: BlockPayload, EXT: Externalities<P>> MalachiteService<P, EXT> {
    /// Bootstrap the service.
    pub async fn new(config: MalachiteConfig, externalities: Arc<EXT>) -> Result<Self> {
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
        let validator_secret_bytes = config.validator_secret.to_bytes();
        let signer = MalachiteSigner::new(private_key);
        let public_key = signer.public_key();
        let address = Address::from_public_key(&public_key);
        let moniker = format!("v-{}", &address.to_string()[..10]);

        tracing::info!(
            target: "ethexe-malachite-core",
            %moniker,
            address = %address,
            listen = %config.listen_addr,
            validators = config.validators.len(),
            role = ?config.role,
            "Bootstrapping Malachite engine",
        );

        let libp2p_keypair = libp2p_keypair_from(&validator_secret_bytes);

        // ---- validator set from config ----
        if config.validators.is_empty() {
            return Err(anyhow::anyhow!("MalachiteConfig::validators is empty"));
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

        // ---- network identity, role-dependent ----
        let identity = match config.role {
            NodeRole::Validator => {
                if !in_set {
                    return Err(anyhow::anyhow!(
                        "NodeRole::Validator: local address {address} not present in MalachiteConfig::validators"
                    ));
                }
                let peer_id_bytes = libp2p_keypair.public().to_peer_id().to_bytes();
                // Sign (validator_pubkey, peer_id_bytes) to bind
                // libp2p identity to the validator's on-chain identity.
                let signing_provider = MalachiteSigner::new(signer.private_key().clone());
                let proof = signing_provider
                    .sign_validator_proof(public_key.to_vec(), peer_id_bytes)
                    .await
                    .map_err(|e| anyhow::anyhow!("signing validator proof: {e:?}"))?;
                let proof_bytes: Bytes = {
                    use malachitebft_app_channel::app::types::codec::Codec;
                    <ScaleCodec as Codec<ValidatorProof<MalachiteCtx>>>::encode(&ScaleCodec, &proof)
                        .map_err(|e| anyhow::anyhow!("encoding validator proof: {e}"))?
                };
                NetworkIdentity::new_validator(
                    moniker.clone(),
                    libp2p_keypair,
                    address.to_string(),
                    proof_bytes,
                )
            }
            NodeRole::FullNode => {
                if in_set {
                    return Err(anyhow::anyhow!(
                        "NodeRole::FullNode: local address {address} must NOT be in MalachiteConfig::validators"
                    ));
                }
                NetworkIdentity::new(moniker.clone(), libp2p_keypair, None)
            }
        };

        // ---- engine ----
        let inner_cfg = build_inner_config(&config, &moniker);
        let ctx = MalachiteCtx::new();
        let consensus_signer = MalachiteSigner::new(signer.private_key().clone());
        let (channels, engine) = EngineBuilder::new(ctx.clone(), inner_cfg)
            .with_default_wal(WalContext::new(wal_path.clone(), ScaleCodec))
            .with_default_network(NetworkContext::new(identity, ScaleCodec))
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
        let store = Store::<P>::open(&store_path).context("opening Store")?;
        let state = State::<P>::new(
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
            if let Err(e) = app::run::<P, EXT>(state, channels, externalities_for_task).await {
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
            _phantom: PhantomData,
        })
    }

    /// Swap the active validator set used at the next height start.
    /// Malachite's `StartHeight` snapshots the set at the height
    /// start, so the current height runs to completion with whatever
    /// it had; the `Finalized` reply then feeds the new set as the
    /// next-height `HeightParams`, keeping the rotation gap-free.
    ///
    /// Caller is responsible for keeping the local validator's pub
    /// key in `validators` while running in [`NodeRole::Validator`]
    /// — we don't carry the role around here. Empty input is rejected.
    pub fn update_validators(&self, validators: Vec<crate::ValidatorEntry>) -> Result<()> {
        if validators.is_empty() {
            return Err(anyhow::anyhow!(
                "MalachiteService::update_validators: empty validators list"
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

impl<P: BlockPayload, EXT: Externalities<P>> Stream for MalachiteService<P, EXT> {
    type Item = anyhow::Error;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.errors_rx.poll_recv(cx)
    }
}

impl<P: BlockPayload, EXT: Externalities<P>> FusedStream for MalachiteService<P, EXT> {
    fn is_terminated(&self) -> bool {
        self.errors_rx.is_closed()
    }
}

impl<P: BlockPayload, EXT: Externalities<P>> MService for MalachiteService<P, EXT> {}

fn build_inner_config(cfg: &MalachiteConfig, moniker: &str) -> InnerNodeConfig {
    let transport = TransportProtocol::Tcp;
    let listen_multiaddr = transport.multiaddr(
        &cfg.listen_addr.ip().to_string(),
        cfg.listen_addr.port() as usize,
    );
    let consensus = ConsensusConfig {
        enabled: true,
        value_payload: ValuePayload::ProposalAndParts,
        queue_capacity: 100,
        p2p: P2pConfig {
            protocol: PubSubProtocol::default(),
            listen_addr: listen_multiaddr,
            persistent_peers: cfg.persistent_peers.clone(),
            discovery: DiscoveryConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        },
    };
    let value_sync = match cfg.env {
        Environment::Production => ValueSyncConfig::default(),
        Environment::Test => ValueSyncConfig {
            status_update_interval: Duration::from_millis(500),
            request_timeout: Duration::from_secs(3),
            parallel_requests: 16,
            batch_size: 32,
            ..Default::default()
        },
    };
    InnerNodeConfig {
        moniker: moniker.to_string(),
        consensus,
        value_sync,
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
