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
use gsigner::{
    Signer,
    schemes::secp256k1::{PublicKey as Secp256k1PublicKey, Secp256k1},
};
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
pub use malachitebft_app::net::Multiaddr;
use malachitebft_app_channel::app::metrics::SharedRegistry;
use malachitebft_app_channel::app::types::core::Height as _;
use malachitebft_app_channel::app::types::Keypair;
use malachitebft_app_channel::{
    ConsensusContext, EngineBuilder, EngineHandle, NetworkContext, NetworkIdentity,
    RequestContext, SigningProviderExt, SyncContext, WalContext,
};
use crate::context::{
    Address, EthexeSigner, Genesis, Height, PrivateKey, PublicKey,
};
use crate::genesis::MalachiteGenesis;

mod app;
mod codec;
mod context;
mod genesis;
mod mempool;
mod quarantine;
mod state;
mod store;
mod streaming;

pub use ethexe_common::mb::{
    ProcessQueuesLimits, ProgressTasksLimits, SequencerBlock, Transaction,
};
pub use crate::mempool::InjectedTxMempool;
pub use libp2p_identity::PeerId;

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

    /// A sequencer block has been committed by the BFT quorum. Carries
    /// both the commit certificate and the block contents — the latter
    /// is needed by the executor (ethexe-compute) to actually run the
    /// transactions.
    #[display(
        "BlockFinalized(height: {}, block_hash: {}, sigs: {}, txs: {})",
        cert.height,
        cert.block_hash,
        cert.signatures.len(),
        block.transactions.len()
    )]
    BlockFinalized {
        cert: CommitCertificate,
        block: SequencerBlock,
    },
}

// ---------------------------------------------------------------------------
// Mempool
// ---------------------------------------------------------------------------

/// Source of injected transactions to pack into the next sequencer block.
///
/// The pool is fed new chain heads via [`Self::set_chain_head`] so it
/// can garbage-collect entries whose `reference_block` has aged past
/// [`ethexe_common::injected::VALIDITY_WINDOW`]. [`Self::fetch`] is
/// non-destructive: a tx is only removed once the MB it ends up in
/// is finalized and passed to [`Self::forget`], at which point the
/// pool must remember the tx hash until it's safe to forget (that's
/// also bounded by `VALIDITY_WINDOW`).
#[async_trait]
pub trait Mempool: Send + Sync + 'static {
    /// Accept a transaction into the pool. Implementations may reject
    /// txs whose `reference_block` has already aged out or whose hash
    /// has recently been committed; the current interface is
    /// fire-and-forget so rejections are swallowed silently (logged).
    fn insert(&self, tx: SignedInjectedTransaction);

    /// Notify the pool of a newly observed Ethereum chain head. Drives
    /// expiration GC for both the pool and the seen-hash dedup table.
    fn set_chain_head(&self, head: SimpleBlockData);

    /// Return a batch of TXs whose `reference_block` is an ancestor
    /// of `head` and that fit within the given gas budget. Non-ancestor
    /// txs stay in the pool — they become eligible again if the chain
    /// reorgs back to their branch.
    async fn fetch(
        &self,
        head: SimpleBlockData,
        gas_budget: u64,
    ) -> Vec<SignedInjectedTransaction>;

    /// Drop the given TXs after they have been included in a committed
    /// (finalized) sequencer block. Implementations should also record
    /// the hashes so subsequent [`Self::insert`] calls for the same tx
    /// are rejected as duplicates, until the ref_block ages out.
    async fn forget(&self, committed: &[SignedInjectedTransaction]);

    /// Block until at least one new transaction is accepted into the
    /// pool. Used by the producer to wake up out of an idle wait the
    /// moment fresh content arrives — without polling.
    ///
    /// The notification is best-effort: spurious wake-ups are
    /// allowed (the producer must always re-check `fetch` after
    /// returning). If the implementation never accepts new txs (e.g.
    /// [`EmptyMempool`]), this future is allowed to be pending forever.
    async fn wait_for_new_tx(&self);
}

/// Always-empty mempool, useful to bring up the service on an idle node.
#[derive(Clone, Default)]
pub struct EmptyMempool;

#[async_trait]
impl Mempool for EmptyMempool {
    fn insert(&self, _tx: SignedInjectedTransaction) {}

    fn set_chain_head(&self, _head: SimpleBlockData) {}

    async fn fetch(
        &self,
        _head: SimpleBlockData,
        _gas_budget: u64,
    ) -> Vec<SignedInjectedTransaction> {
        Vec::new()
    }

    async fn forget(&self, _committed: &[SignedInjectedTransaction]) {}

    async fn wait_for_new_tx(&self) {
        // Empty pool never accepts a tx — pend forever so the
        // producer's select races only against chain_head_rx.
        std::future::pending().await
    }
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

    /// Multiaddrs the local node should keep persistent connections
    /// to (analogue of `--bootnodes` for `ethexe-network`). Each entry
    /// must include a `/p2p/<peer_id>` suffix so the swarm knows
    /// who to expect on the other side. Discovery is currently off,
    /// so multi-validator deployments need every node listed in here
    /// (or at least transitively reachable).
    pub persistent_peers: Vec<Multiaddr>,
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
            persistent_peers: Vec::new(),
        }
    }

    /// Replace the Malachite libp2p listen address.
    #[must_use]
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = addr;
        self
    }

    /// Replace the Malachite persistent peers list.
    #[must_use]
    pub fn with_persistent_peers(mut self, peers: Vec<Multiaddr>) -> Self {
        self.persistent_peers = peers;
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
            persistent_peers: cfg.persistent_peers.clone(),
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
// Key extraction
// ---------------------------------------------------------------------------

/// Pull the raw 32-byte secp256k1 secret for `validator_pub_key` out
/// of the shared [`gsigner::Signer`]. Same secret signs Malachite
/// consensus votes (via [`EthexeSigner`]) and on-chain commitments
/// (via ethexe-ethereum). The libp2p peer_id is intentionally **not**
/// derived from this secret directly — see [`derive_libp2p_secret`].
fn export_validator_secret(
    signer: &Signer<Secp256k1>,
    validator_pub_key: Secp256k1PublicKey,
) -> Result<[u8; 32]> {
    let priv_key = signer
        .private_key(validator_pub_key)
        .context("exporting validator private key from gsigner keyring")?;
    Ok(priv_key.to_bytes())
}

/// Domain-separated secp256k1 secret for the Malachite libp2p swarm.
///
/// We never reuse the validator key as a libp2p peer_id source: the
/// two layers serve different purposes (transport vs consensus) and
/// having the same peer_id across two independent libp2p swarms in a
/// single process is bad hygiene (operability, observability,
/// future-proofing). Instead, we deterministically hash the master
/// secret with a fixed domain tag — operators still manage a single
/// key, but the libp2p peer_id ends up independent of the validator
/// address.
///
/// The probability that the resulting 32 bytes are not a valid
/// secp256k1 scalar (i.e., are zero or ≥ curve order) is on the order
/// of 2^-128, so we panic on the (essentially impossible) failure.
pub fn derive_libp2p_secret(validator_secret: &[u8; 32]) -> [u8; 32] {
    const DOMAIN: &[u8] = b"ethexe-malachite-libp2p:v1:";
    let mut hasher = Keccak256::new();
    hasher.update(DOMAIN);
    hasher.update(validator_secret);
    hasher.finalize().into()
}

/// Compute the libp2p [`PeerId`] of the Malachite swarm associated
/// with a given validator secret — without spinning up the engine.
///
/// Useful for offline tooling: operators preparing
/// `--malachite-persistent-peer` multiaddrs for a multi-validator
/// deployment can run this against each validator's keystore to
/// produce the `/p2p/<peer_id>` suffix without booting the node.
///
/// The derivation matches the one [`MalachiteService::new`] performs
/// internally, so the offline-computed peer_id will be identical to
/// what the running service announces.
pub fn malachite_libp2p_peer_id(validator_secret: &[u8; 32]) -> PeerId {
    let mut derived = derive_libp2p_secret(validator_secret);
    let secret = libp2p_identity::secp256k1::SecretKey::try_from_bytes(&mut derived)
        .expect("derived libp2p secret is a valid secp256k1 scalar");
    for byte in derived.iter_mut() {
        *byte = 0;
    }
    let kp = libp2p_identity::secp256k1::Keypair::from(secret);
    Keypair::from(kp).public().to_peer_id()
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
    /// Bootstrap the Malachite engine + app task.
    ///
    /// Parameters:
    /// - `signer` — shared ethexe key manager; the raw secp256k1
    ///   secret for `validator_pub_key` is extracted once here to
    ///   drive Malachite signing, libp2p identity, and on-chain
    ///   commitments off the same key.
    /// - `validator_pub_key` — this node's validator public key;
    ///   must appear in the genesis validator set at `home_dir`.
    /// - `db` — shared ethexe [`Database`] for quarantine walks.
    ///
    /// The engine runs in the background; `Stream::poll_next` then
    /// forwards [`MalachiteEvent`]s out of the app task.
    pub async fn new(
        config: MalachiteConfig,
        db: Database,
        signer: Signer<Secp256k1>,
        validator_pub_key: Secp256k1PublicKey,
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
        // The validator's signing key is what signs Malachite votes
        // and on-chain commitments. The libp2p peer_id, however, is
        // a *transport-level* identity — running two libp2p swarms
        // (ethexe-network on QUIC/20333, malachite on TCP/20334) under
        // the same peer_id is non-standard and confuses observability
        // / future tooling, even though there's no functional clash.
        //
        // So we keep a single master secret (the validator key) but
        // domain-separate-derive an independent secp256k1 secret for
        // the malachite libp2p swarm. Operators still manage one key;
        // ethexe-network and malachite end up with distinct peer_ids.
        let secret_bytes = export_validator_secret(&signer, validator_pub_key)
            .context("extracting validator secret for Malachite")?;
        let private_key = PrivateKey::from_slice(&secret_bytes)
            .map_err(|e| anyhow::anyhow!("constructing ECDSA private key: {e}"))?;
        let public_key: PublicKey = private_key.public_key();
        let address = Address::from_public_key(&public_key);
        let signing_provider = EthexeSigner::new(private_key.clone());

        let libp2p_keypair: Keypair = {
            let mut derived = derive_libp2p_secret(&secret_bytes);
            let secret = libp2p_identity::secp256k1::SecretKey::try_from_bytes(&mut derived)
                .expect("derived libp2p secret is a valid secp256k1 scalar");
            // Zero our temp copy; the on-chain key is still in the
            // gsigner keyring, the derived libp2p key now lives only
            // inside the libp2p Keypair.
            for byte in derived.iter_mut() {
                *byte = 0;
            }
            let inner = libp2p_identity::secp256k1::Keypair::from(secret);
            Keypair::from(inner)
        };
        let peer_id_bytes = libp2p_keypair.public().to_peer_id().to_bytes();
        // Bind libp2p peer_id to the validator's on-chain identity:
        // the validator signs (validator_pubkey, peer_id_bytes), peers
        // verify against the validator's public key from genesis. Even
        // though peer_id != address now, the proof stitches them
        // together so a peer can't impersonate a validator over libp2p.
        let proof = signing_provider
            .sign_validator_proof(public_key.to_vec(), peer_id_bytes)
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
            libp2p_keypair,
            address.to_string(),
            proof_bytes,
        );

        // ---- genesis validator set ---------------------------------------
        let genesis_path = config.home_dir.join("genesis.json");
        let genesis_raw = MalachiteGenesis::load(&genesis_path)
            .with_context(|| format!("loading Malachite genesis from {}", genesis_path.display()))?;
        if !genesis_raw
            .validators
            .iter()
            .any(|v| v.address == address)
        {
            return Err(anyhow::anyhow!(
                "local validator address {address} not found in genesis at {}",
                genesis_path.display()
            ));
        }
        let validator_set = genesis_raw.to_validator_set();
        let genesis = Genesis { validator_set };

        // ---- engine -------------------------------------------------------
        let inner_cfg = build_inner_config(&config);
        let ctx = EthexeContext::new();

        // Keep an independent signing provider for the consensus
        // engine; the `State` keeps its own copy for streaming
        // proposal fins. Both wrap the same private key.
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

        let max_decided_height = store.max_decided_value_height().await;
        let start_height = max_decided_height
            .map(|h| h.increment())
            .unwrap_or(Height::INITIAL);

        // On a fresh node, no MB has been committed yet → parent for
        // the very first MB is zero. On restart, recover the last
        // finalized MB's hash from the malachite store so the next
        // proposal's `parent` links correctly.
        let latest_finalized_mb_hash = match max_decided_height {
            Some(h) => store
                .get_decided_value(h)
                .await
                .ok()
                .flatten()
                .map(|dv| dv.value.block.hash())
                .unwrap_or_default(),
            None => H256::zero(),
        };

        let state = State::new(
            ctx,
            signing_provider,
            genesis,
            address,
            start_height,
            store,
            db,
            config.canonical_quarantine,
            latest_finalized_mb_hash,
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

