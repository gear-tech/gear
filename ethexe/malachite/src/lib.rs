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

//! # Ethexe Malachite
//!
//! Consensus service powered by the Malachite BFT engine (Tendermint
//! variant). Orders injected transactions into a stream of
//! deterministic [`SequencerBlock`]s, each anchored to an Ethereum block
//! that has passed the ethexe quarantine.
//!
//! ## Inputs
//! - A shared [`ethexe_db::Database`] passed in at construction —
//!   used to walk `parent_hash` links when computing or verifying the
//!   quarantine anchor.
//! - [`MalachiteService::receive_new_chain_head`] — the latest
//!   Ethereum block observed by the node. Only the newest value is
//!   retained (no history); it is the reference point for picking
//!   and validating the quarantine anchor.
//! - A [`Mempool`] passed in at construction, sampled from whenever the
//!   node is the proposer.
//!
//! ## Outputs (`Stream<Item = Result<MalachiteEvent>>`)
//! - [`MalachiteEvent::BlockProposal`] — a new sequencer block has been
//!   produced (if we are proposer) or validated from a peer.
//! - [`MalachiteEvent::BlockFinalized`] — a sequencer block has been
//!   committed by the BFT quorum.
//!
//! ## Genesis
//! [`derive_chain_id`] maps the ethexe genesis block hash (the hash of
//! the Ethereum block at which the Router was deployed) to a
//! deterministic 32-byte Malachite chain id, so two nodes on the same
//! ethexe deployment agree on the chain id without any extra
//! configuration.
//!
//! ## Internals
//!
//! On construction the service spawns two background tasks:
//! - the Malachite engine (managed by [`EngineHandle`]),
//! - an app-channel event loop in [`app::run`] which translates
//!   Malachite's `AppMsg`s into our `MalachiteEvent`s and persists
//!   decisions to the [`store::Store`] backed by `ethexe-db`.
//!
//! The outer [`Stream`] impl is a thin forwarder over an `mpsc`.

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use bytes::Bytes;
use ethexe_common::{SimpleBlockData, injected::SignedInjectedTransaction};
use ethexe_db::Database;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use rand::rngs::OsRng;
use sha3::{Digest, Keccak256};
use std::{
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::Instrument;

use malachitebft_app_channel::app::config::{
    ConsensusConfig, DiscoveryConfig, LoggingConfig, MetricsConfig, NodeConfig, P2pConfig,
    PubSubProtocol, RuntimeConfig, TransportProtocol, ValuePayload, ValueSyncConfig,
};
use malachitebft_app_channel::app::metrics::SharedRegistry;
use malachitebft_app_channel::app::types::core::Height as _;
use malachitebft_app_channel::app::types::Keypair;
use malachitebft_app_channel::{
    ConsensusContext, EngineBuilder, EngineHandle, NetworkContext, NetworkIdentity,
    RequestContext, SigningProviderExt, SyncContext, WalContext,
};
use crate::context::{
    Address, EthexeSigner, Genesis, Height, PrivateKey, Validator, ValidatorSet,
};

mod app;
mod block;
mod codec;
mod context;
mod mempool;
mod quarantine;
mod state;
mod store;
mod streaming;

pub use crate::block::{
    ProcessQueuesLimits, ProgressTasksLimits, SequencerBlock, Transaction,
};
pub use crate::mempool::InjectedTxMempool;

use crate::codec::JsonCodec;
use crate::context::EthexeContext;
use crate::state::State;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Block / certificate (public types used by the Stream consumer)
// ---------------------------------------------------------------------------

/// Commit certificate — a finalized block together with the aggregated
/// precommit signatures that authorize it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitCertificate {
    pub height: u64,
    pub block_hash: H256,
    pub signatures: Vec<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// Output event stream of the Malachite service. `height` is the
/// Malachite sequencer height at which the block was produced /
/// finalized — it's reported here (rather than embedded inside the
/// block) because the block itself is just an ordered transaction
/// stream with no self-referential height field.
#[derive(Debug, Clone, derive_more::Display)]
pub enum MalachiteEvent {
    /// A new sequencer block has been produced (if this node is the
    /// proposer) or received and validated from a peer.
    #[display("BlockProposal(height: {}, txs: {})", height, block.transactions.len())]
    BlockProposal {
        height: u64,
        block: SequencerBlock,
    },

    /// A sequencer block has been committed by the BFT quorum.
    #[display(
        "BlockFinalized(height: {}, block_hash: {}, sigs: {})",
        _0.height,
        _0.block_hash,
        _0.signatures.len()
    )]
    BlockFinalized(CommitCertificate),
}

// ---------------------------------------------------------------------------
// Mempool
// ---------------------------------------------------------------------------

/// Source of injected transactions to pack into the next sequencer block.
#[async_trait]
pub trait Mempool: Send + Sync + 'static {
    /// Accept a transaction into the pool.
    fn insert(&self, tx: SignedInjectedTransaction);

    /// Return a batch of TXs that fit within the given gas budget.
    async fn fetch(&self, gas_budget: u64) -> Vec<SignedInjectedTransaction>;

    /// Drop the given TXs after they have been included in a committed block.
    async fn forget(&self, committed: &[SignedInjectedTransaction]);
}

/// Always-empty mempool, useful to bring up the service on an idle node.
#[derive(Clone, Default)]
pub struct EmptyMempool;

#[async_trait]
impl Mempool for EmptyMempool {
    fn insert(&self, _tx: SignedInjectedTransaction) {}

    async fn fetch(&self, _gas_budget: u64) -> Vec<SignedInjectedTransaction> {
        Vec::new()
    }

    async fn forget(&self, _committed: &[SignedInjectedTransaction]) {}
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Deterministic 32-byte Malachite chain id.
pub type ChainId = [u8; 32];

#[derive(Clone, Debug)]
pub struct MalachiteConfig {
    /// Human-readable node name (used by Malachite logs & identify).
    pub moniker: String,

    /// Gas allowance per block.
    pub gas_allowance: u64,

    /// Number of canonical descendants an Ethereum block must have
    /// before it is considered out of quarantine and safe to anchor a
    /// sequencer block to. Matches
    /// [`ethexe_compute::ComputeConfig::canonical_quarantine`].
    pub canonical_quarantine: u8,

    /// Deterministic chain id derived from the ethexe genesis.
    pub chain_id: ChainId,

    /// Local libp2p listen address for the Malachite swarm.
    pub listen_addr: SocketAddr,

    /// Directory where Malachite stores its WAL and block DB.
    pub home_dir: PathBuf,
}

impl MalachiteConfig {
    pub const DEFAULT_GAS_ALLOWANCE: u64 = 1_000_000_000;
    /// Default matches [`ethexe_common::gear::CANONICAL_QUARANTINE`].
    pub const DEFAULT_CANONICAL_QUARANTINE: u8 =
        ethexe_common::gear::CANONICAL_QUARANTINE;
    /// Sits right next to `ethexe-network`'s default (20333/udp for
    /// QUIC) so operators can open a single range of ports. Note the
    /// protocol difference: Malachite currently binds a TCP listener.
    pub const DEFAULT_LISTEN_ADDR: SocketAddr = SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        20334,
    );

    /// Build a config with sane defaults from the ethexe genesis hash
    /// and the node's home directory. The Malachite listen address is
    /// left at [`Self::DEFAULT_LISTEN_ADDR`] — override it with
    /// [`Self::with_listen_addr`] before passing to the service.
    pub fn from_ethexe_genesis(ethexe_genesis_block_hash: H256, home_dir: PathBuf) -> Self {
        Self {
            moniker: "ethexe-malachite".to_string(),
            gas_allowance: Self::DEFAULT_GAS_ALLOWANCE,
            canonical_quarantine: Self::DEFAULT_CANONICAL_QUARANTINE,
            chain_id: derive_chain_id(ethexe_genesis_block_hash),
            listen_addr: Self::DEFAULT_LISTEN_ADDR,
            home_dir,
        }
    }

    /// Replace the Malachite libp2p listen address.
    #[must_use]
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = addr;
        self
    }
}

/// Internal config that Malachite's EngineBuilder actually consumes.
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

fn build_inner_config(cfg: &MalachiteConfig) -> InnerNodeConfig {
    let transport = TransportProtocol::Tcp;
    let listen_multiaddr = transport.multiaddr(
        &cfg.listen_addr.ip().to_string(),
        cfg.listen_addr.port() as usize,
    );

    let consensus = ConsensusConfig {
        enabled: true,
        // `ProposalAndParts` is what the upstream channel example uses;
        // `PartsOnly` would be more network-efficient but doesn't
        // properly carry `valid_round` for `Init` messages.
        value_payload: ValuePayload::ProposalAndParts,
        queue_capacity: 100,
        p2p: P2pConfig {
            protocol: PubSubProtocol::default(),
            listen_addr: listen_multiaddr,
            persistent_peers: Vec::new(),
            discovery: DiscoveryConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        },
    };

    InnerNodeConfig {
        moniker: cfg.moniker.clone(),
        consensus,
        value_sync: ValueSyncConfig::default(),
        logging: LoggingConfig::default(),
        metrics: MetricsConfig::default(),
        runtime: RuntimeConfig::default(),
    }
}

// ---------------------------------------------------------------------------
// Genesis derivation
// ---------------------------------------------------------------------------

/// Derive a deterministic 32-byte Malachite chain id from the ethexe
/// genesis block hash. Different ethexe deployments produce different
/// chain ids, which prevents Malachite messages from being replayed
/// across chains, while every node on the same ethexe deployment
/// agrees on the same chain id without any extra configuration.
pub fn derive_chain_id(ethexe_genesis_block_hash: H256) -> ChainId {
    const DOMAIN: &[u8] = b"ethexe-malachite-chain-id:v1:";
    let mut h = Keccak256::new();
    h.update(DOMAIN);
    h.update(ethexe_genesis_block_hash.as_bytes());
    let out = h.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&out);
    id
}

// ---------------------------------------------------------------------------
// Node-key persistence
// ---------------------------------------------------------------------------

/// Read the Malachite ed25519 node key from `home_dir/node_key.json`,
/// generating and persisting a fresh one on first boot. Restarts keep
/// the same libp2p peer id so peers don't have to re-discover this
/// node every time it comes back up.
fn load_or_generate_node_key(home_dir: &std::path::Path) -> Result<PrivateKey> {
    let key_path = home_dir.join("node_key.json");
    if key_path.exists() {
        let raw = std::fs::read_to_string(&key_path)
            .with_context(|| format!("reading {}", key_path.display()))?;
        let key: PrivateKey = serde_json::from_str(&raw)
            .with_context(|| format!("decoding {}", key_path.display()))?;
        tracing::debug!(target: "ethexe::malachite", path = %key_path.display(), "loaded node key");
        Ok(key)
    } else {
        let key = PrivateKey::generate(OsRng);
        let encoded = serde_json::to_string_pretty(&key).context("encoding node key")?;
        // Ensure the directory exists before we try to write into it —
        // the engine bootstrap creates WAL dirs elsewhere; we don't
        // want to depend on that ordering.
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating node-key dir {}", parent.display())
            })?;
        }
        std::fs::write(&key_path, encoded)
            .with_context(|| format!("writing {}", key_path.display()))?;
        tracing::info!(
            target: "ethexe::malachite",
            path = %key_path.display(),
            "generated new node key",
        );
        // POSIX-only: tighten file perms on Unix so key is owner-readable only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_path, perms)?;
        }
        Ok(key)
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Malachite-backed consensus service.
pub struct MalachiteService {
    events_rx: mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    chain_head_tx: mpsc::UnboundedSender<SimpleBlockData>,
    mempool: Arc<dyn Mempool>,

    #[allow(dead_code)]
    engine: EngineHandle,
    #[allow(dead_code)]
    app_handle: JoinHandle<()>,
}

impl MalachiteService {
    /// Bootstrap the Malachite engine + app task. The engine runs in the
    /// background; `Stream::poll_next` then forwards
    /// [`MalachiteEvent`]s out of the app task.
    ///
    /// `db` is the shared ethexe [`Database`], used for walking
    /// `parent_hash` links along the canonical chain. The current
    /// chain head itself is fed in later via
    /// [`Self::receive_new_chain_head`] so both producer and
    /// validators work off the exact block received from the observer
    /// event stream rather than `DBGlobals::latest_synced_block`.
    pub async fn new(
        config: MalachiteConfig,
        db: Database,
        mempool: Arc<dyn Mempool>,
    ) -> Result<Self> {
        tracing::info!(
            target: "ethexe::malachite",
            moniker = %config.moniker,
            chain_id = %hex::encode(config.chain_id),
            listen = %config.listen_addr,
            "Bootstrapping Malachite engine",
        );

        std::fs::create_dir_all(&config.home_dir)
            .with_context(|| format!("creating Malachite home dir {:?}", config.home_dir))?;
        let wal_path = config.home_dir.join("wal").join("consensus.wal");
        std::fs::create_dir_all(
            wal_path
                .parent()
                .expect("wal path has a parent"),
        )?;

        let db_path = config.home_dir.join("store.db");

        // ---- keys & identity ---------------------------------------------
        // Persist the ed25519 node key at `home_dir/node_key.json` so the
        // peer id is stable across restarts. Generated on first boot.
        let private_key = load_or_generate_node_key(&config.home_dir)
            .context("loading/generating Malachite node key")?;
        let public_key = private_key.public_key();
        let address = Address::from_public_key(&public_key);
        let signing_provider = EthexeSigner::new(private_key.clone());

        let keypair = Keypair::ed25519_from_bytes(private_key.inner().to_bytes())
            .expect("valid ed25519 keypair");
        let peer_id_bytes = keypair.public().to_peer_id().to_bytes();
        let proof = signing_provider
            .sign_validator_proof(public_key.as_bytes().to_vec(), peer_id_bytes)
            .await
            .map_err(|e| anyhow::anyhow!("failed to sign validator proof: {e:?}"))?;
        let proof_bytes: Bytes = {
            use malachitebft_app_channel::app::types::codec::Codec;
            <JsonCodec as Codec<malachitebft_core_types::ValidatorProof<EthexeContext>>>::encode(
                &JsonCodec, &proof,
            )
                .map_err(|e| anyhow::anyhow!("failed to encode validator proof: {e}"))?
        };
        let identity = NetworkIdentity::new_validator(
            config.moniker.clone(),
            keypair,
            address.to_string(),
            proof_bytes,
        );

        // ---- genesis (single validator) ----------------------------------
        let validator_set = ValidatorSet::new(vec![Validator::new(public_key, 1)]);
        let genesis = Genesis { validator_set };

        // ---- engine -------------------------------------------------------
        let inner_cfg = build_inner_config(&config);
        let ctx = EthexeContext::new();

        // Ed25519Provider doesn't impl Clone, so make two providers that
        // share the same private key — one for Malachite's consensus,
        // one retained in `State` for signing proposal fins.
        let consensus_signer = EthexeSigner::new(private_key.clone());
        let (channels, engine) = EngineBuilder::new(ctx.clone(), inner_cfg)
            .with_default_wal(WalContext::new(wal_path, JsonCodec))
            .with_default_network(NetworkContext::new(identity, JsonCodec))
            .with_default_consensus(ConsensusContext::new(address, consensus_signer))
            .with_default_sync(SyncContext::new(JsonCodec))
            .with_default_request(RequestContext::new(100))
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("building Malachite engine: {e}"))?;

        // Side-effect: register the metrics sub-tree so the Prometheus
        // layer doesn't lose them silently. We don't expose them yet.
        let _registry = SharedRegistry::global().with_moniker(&config.moniker);

        // ---- store --------------------------------------------------------
        let store = Store::open(&db_path)
            .await
            .context("opening Malachite store")?;

        // ---- app task -----------------------------------------------------
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (chain_head_tx, chain_head_rx) = mpsc::unbounded_channel();

        let start_height = store
            .max_decided_value_height()
            .await
            .map(|h| h.increment())
            .unwrap_or(Height::INITIAL);

        let state = State::new(
            ctx,
            signing_provider,
            genesis,
            address,
            start_height,
            store,
            db,
            config.canonical_quarantine,
        );

        let gas_allowance = config.gas_allowance;
        let mempool_for_service = Arc::clone(&mempool);
        let span = tracing::error_span!("ethexe::malachite::app", moniker = %config.moniker);
        let app_handle = tokio::spawn(
            async move {
                if let Err(e) = app::run(
                    state,
                    channels,
                    mempool,
                    gas_allowance,
                    chain_head_rx,
                    events_tx,
                )
                .await
                {
                    tracing::error!(target: "ethexe::malachite", error = %e, "App task terminated");
                }
            }
            .instrument(span),
        );

        Ok(Self {
            events_rx,
            chain_head_tx,
            mempool: mempool_for_service,
            engine,
            app_handle,
        })
    }

    /// Pass an injected transaction into the Malachite mempool. The
    /// producer on this node will pull from the same pool when
    /// assembling the next sequencer block.
    pub fn receive_injected_transaction(&self, tx: SignedInjectedTransaction) {
        self.mempool.insert(tx);
    }

    /// Feed the latest observer-delivered chain head into the
    /// Malachite app. Only the newest value is retained — intermediate
    /// heads are harmlessly overwritten.
    pub fn receive_new_chain_head(&mut self, head: SimpleBlockData) {
        if self.chain_head_tx.send(head).is_err() {
            tracing::warn!(target: "ethexe::malachite", "app task closed, chain head dropped");
        }
    }
}

impl Stream for MalachiteService {
    type Item = Result<MalachiteEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events_rx.poll_recv(cx)
    }
}

impl FusedStream for MalachiteService {
    fn is_terminated(&self) -> bool {
        self.events_rx.is_closed()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_id_is_deterministic_and_domain_separated() {
        let h1 = H256::from_low_u64_be(1);
        let h2 = H256::from_low_u64_be(2);
        assert_eq!(derive_chain_id(h1), derive_chain_id(h1));
        assert_ne!(derive_chain_id(h1), derive_chain_id(h2));
        assert_ne!(derive_chain_id(h1), h1.to_fixed_bytes());
    }

    // NOTE: an end-to-end test that actually spins up the engine lives
    // in `ethexe-service` integration tests — we avoid doing it here
    // because it pulls in the whole Malachite libp2p stack and
    // substantially slows unit-test feedback.

    // Static check: the public types are stable.
    #[allow(dead_code)]
    fn _api_shape(
        _ev: MalachiteEvent,
        _block: SequencerBlock,
        _cert: CommitCertificate,
        _mp: EmptyMempool,
        _cfg: MalachiteConfig,
    ) {
    }
}

