// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Vara.eth service
//! Top-level runtime service for an ethexe node.
//!
//! ## Responsibilities
//! This crate provides the top-level [`Service`] orchestrator for ethexe.
//! The service owns all subservices and drives them through a single async event loop.
//!
//! ## Event Loop
//! The subservices in [`Service`] communicate with each other via the [`Event`] enum.
//!
//! In [`Service::run`], the service uses the [tokio::select] macro to poll
//! event streams (see [`futures::Stream`]) and route events to the appropriate subservice.
//!
//! ## Configuration And Startup
//! [`Service::new`] takes a [`Config`] on startup.
//! [`Config`] contains all configuration options required to create the subservices.
//!
//! In the general case, [`Service::new`] is called from the `ethexe-cli` crate,
//! where [`Config`] is parsed from command-line arguments or a configuration file.
//!
//! ## Testing
//! Integration tests for this crate live in `src/tests`.
//! They use `TestEnv` to prepare an Anvil-based or external Ethereum environment,
//! initialize an in-memory database, and construct test nodes.
//!
//! Each node runs [`Service`] using `Service::new_from_parts`.
//! Tests observe service behavior through `TestingEvent` streams, which mirror the
//! internal [`Event`] flow and allow waiting for startup, block sync,
//! MB processing, network activity, and RPC requests.

use crate::{
    config::{Config, ConfigPublicKey},
    pending_tx::PendingNetworkInjectedTx,
};
use alloy::{
    eips::BlockId,
    node_bindings::{Anvil, AnvilInstance},
    providers::{ProviderBuilder, RootProvider, ext::AnvilApi},
    rpc::types::anvil::Metadata,
};
use anyhow::{Context, Result, bail};
use ethexe_blob_loader::{BlobLoader, BlobLoaderEvent, BlobLoaderService, ConsensusLayerConfig};
use ethexe_common::{
    CodeAndIdUnchecked, PromiseEmissionMode,
    db::{GlobalsStorageRW, MbStorageRO},
    injected::{CompactPromise, InjectedTransactionAcceptance, Receipt},
    network::VerifiedValidatorMessage,
};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_consensus::{ConsensusEvent, ConsensusService, ValidatorConfig, ValidatorService};
use ethexe_db::{
    Database, GenesisInitializer, InitConfig, RawDatabase, RocksDatabase, dump::StateDump,
};
use ethexe_ethereum::{EthereumBuilder, deploy::EthereumDeployer, router::RouterQuery};
use ethexe_malachite::{
    InjectedTxMempool, MalachiteEvent, MalachiteServiceConfig, MalachiteServiceStarter,
    ValidatorEntry,
};
use ethexe_network::{NetworkEvent, NetworkRuntimeConfig, NetworkService, TransportType};
use ethexe_observer::{ObserverConfig, ObserverEvent, ObserverService, utils::BlockLoader};
use ethexe_processor::{ProcessedCodeInfo, Processor, ProcessorConfig, ValidCodeInfo};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcServer};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use futures::{FutureExt, StreamExt};
use gprimitives::CodeId;
use gsigner::secp256k1::{Address, PrivateKey, PublicKey, Secp256k1SignerExt, Signer};
use std::{
    collections::{BTreeMap, HashMap},
    num::NonZero,
    path::PathBuf,
    pin::Pin,
    time::Duration,
};
use tokio::sync::oneshot;

pub mod config;

mod fast_sync;
mod pending_tx;
#[cfg(test)]
mod tests;

#[derive(Debug, derive_more::From)]
pub enum Event {
    Compute(ComputeEvent),
    Consensus(ConsensusEvent),
    Malachite(MalachiteEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Rpc(RpcEvent),
    Prometheus(PrometheusEvent),
}

/// Build the Malachite validator set from the on-chain validator
/// list (in router order) by looking each address up in the
/// `address -> public key` table loaded from the
/// `--validators-malachite-pub-keys` JSON file.
///
/// Voting power is fixed at 1 — Malachite quorum is `> 2/3` of the
/// total, which under uniform weights matches the Router's
/// signature threshold. If/when the Router exposes per-validator
/// stake, the lookup here is the natural place to plumb it through.
fn build_malachite_validator_set(
    on_chain_validators: impl IntoIterator<Item = Address>,
    pub_keys: &BTreeMap<Address, PublicKey>,
) -> Result<Vec<ValidatorEntry>> {
    on_chain_validators
        .into_iter()
        .map(|addr| {
            let pub_key = pub_keys.get(&addr).copied().with_context(|| {
                format!(
                    "validator address {addr} has no entry in --validators-malachite-pub-keys; \
                     every on-chain validator must be present in the table"
                )
            })?;
            Ok(ValidatorEntry {
                public_key: pub_key,
                voting_power: 1,
            })
        })
        .collect()
}

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    blob_loader: Box<dyn BlobLoaderService>,
    compute: ComputeService,
    /// `None` for connect (non-validator) nodes.
    consensus: Option<Pin<Box<dyn ConsensusService>>>,
    malachite_starter: MalachiteServiceStarter,
    signer: Signer,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcServer>,

    fast_sync: bool,
    validator_pub_key: Option<PublicKey>,

    /// When set, `run` performs `MalachiteService::shutdown` on signal.
    shutdown_rx: Option<oneshot::Receiver<()>>,

    #[cfg(test)]
    sender: tests::utils::TestingEventSender,
}

impl Service {
    /// Number of reserved dev accounts (deployer, validator).
    const RESERVED_DEV_ACCOUNTS: u32 = 2;
    /// Expected Foundry toolchain commit sha.
    const FOUNDRY_TOOLCHAIN_COMMIT_SHA: &str = "f83bad912a9dba7bf0371def1e70bb1896048356";
    /// Expected Foundry toolchain version.
    const FOUNDRY_TOOLCHAIN_VERSION: &str = "1.7.0";

    fn check_foundry_toolchain_version(client_commit_sha: Option<String>) -> Result<()> {
        if let Some(client_commit_sha) = client_commit_sha
            && client_commit_sha != Self::FOUNDRY_TOOLCHAIN_COMMIT_SHA
        {
            bail!(
                "Commit hash mismatch in Foundry toolchain! Please use: `foundryup --install {version} --force`.",
                version = Self::FOUNDRY_TOOLCHAIN_VERSION,
            );
        }

        Ok(())
    }

    pub async fn configure_dev_environment(
        key_path: PathBuf,
        block_time: Duration,
        pre_funded_accounts: u32,
    ) -> Result<(AnvilInstance, PublicKey, Address)> {
        let signer = Signer::fs(key_path).with_context(|| "failed to open dev keystore")?;

        let pre_funded_accounts = pre_funded_accounts
            .checked_add(Self::RESERVED_DEV_ACCOUNTS)
            .with_context(|| {
                format!("number of pre-funded accounts is too large: {pre_funded_accounts}")
            })?;
        let anvil = Anvil::new()
            .arg("--accounts")
            .arg(pre_funded_accounts.to_string())
            .port(8545_u16)
            .spawn();

        let mut it = anvil
            .keys()
            .iter()
            .map(|key| {
                let seed = key.to_bytes().into();
                PrivateKey::from_seed(seed).expect("anvil should provide valid secp256k1 key")
            })
            .zip(anvil.addresses().iter().map(|addr| Address::from(*addr)));

        let (deployer_private_key, deployer_address) = it.next().expect("infallible");
        let (validator_private_key, validator_address) = it.next().expect("infallible");

        signer.import(deployer_private_key.clone())?;
        let validator_public_key = signer.import(validator_private_key.clone())?;

        log::info!("🔐 Available Accounts:");

        log::info!("     Deployer:  {deployer_address} {deployer_private_key}");
        log::info!("     Validator: {validator_address} {validator_private_key}");

        for ((sender_private_key, sender_address), i) in it.clone().zip(1_usize..) {
            log::info!("     Sender:    {sender_address} {sender_private_key} (#{i})");
            signer.import(sender_private_key)?;
        }

        let ethereum =
            EthereumDeployer::new(&anvil.ws_endpoint(), signer.clone(), deployer_address)
                .await
                .unwrap()
                .with_validators(vec![validator_address].try_into().unwrap())
                .deploy()
                .await?;

        let provider: RootProvider = ProviderBuilder::default()
            .connect(anvil.ws_endpoint().as_str())
            .await?;

        let Metadata {
            client_commit_sha, ..
        } = provider.anvil_metadata().await?;

        Self::check_foundry_toolchain_version(client_commit_sha)?;

        const ETHER: u128 = 1_000_000_000_000_000_000;
        let balance = 10_000 * ETHER;
        let balance = balance.try_into().expect("infallible");

        let wvara = ethereum.wrapped_vara();
        let decimals = wvara.query().decimals().await?;
        let amount = 500_000 * (10_u128.pow(decimals as _));

        provider
            .anvil_set_balance(deployer_address.into(), balance)
            .await?;

        provider
            .anvil_set_balance(validator_address.into(), balance)
            .await?;

        wvara.mint(validator_address.into(), amount).await?;

        for (_, sender_address) in it {
            provider
                .anvil_set_balance(sender_address.into(), balance)
                .await?;

            wvara.mint(sender_address.into(), amount).await?;
        }

        provider
            .anvil_set_interval_mining(block_time.as_secs())
            .await?;

        Ok((anvil, validator_public_key, ethereum.router().address()))
    }

    pub async fn new(config: &Config) -> Result<Self> {
        let rocks_db = RocksDatabase::open(
            config
                .node
                .database_path_for(config.ethereum.router_address),
        )
        .with_context(|| "failed to open database")?;

        let genesis_initializer: Option<Box<dyn GenesisInitializer>> =
            match &config.node.genesis_state_dump {
                Some(path) => {
                    log::info!("Using genesis state dump: {}", path.display());
                    Some(Box::new(GenesisInitializerFromFile::new(path.clone())?))
                }
                None => None,
            };

        let db = ethexe_db::initialize_db(
            InitConfig {
                ethereum_rpc: config.ethereum.rpc.clone(),
                router_address: config.ethereum.router_address,
                slot_duration_secs: config.ethereum.block_time.as_secs(),
                genesis_initializer,
            },
            RawDatabase::from_one(&rocks_db),
        )
        .await?;

        if config.node.db_cleanup {
            log::info!("Pruning old MB schedules (--db-cleanup)...");
            // Safety: nothing else touches the database yet — services
            // are constructed below.
            let pruned = unsafe { db.cleanup() };
            log::info!("MB schedule cleanup done, pruned schedules of {pruned} MBs");
        }

        let consensus_config = ConsensusLayerConfig {
            ethereum_rpc: config.ethereum.rpc.clone(),
            ethereum_beacon_rpc: config.ethereum.beacon_rpc.clone(),
            beacon_block_time: config.ethereum.block_time,
            attempts: const { NonZero::<u8>::new(3).unwrap() },
        };
        let blob_loader = BlobLoader::new(db.clone(), consensus_config)
            .await
            .context("failed to create blob loader")?
            .into_box();

        let prometheus = if let Some(config) = config.prometheus.clone() {
            Some(PrometheusService::new(config, db.clone())?)
        } else {
            None
        };

        let rpc = config
            .rpc
            .clone()
            .map(|config| RpcServer::new(config, db.clone()));

        let observer = ObserverService::new(
            db.clone(),
            ObserverConfig {
                rpc: &config.ethereum.rpc,
                max_sync_depth: Some(config.node.eth_max_sync_depth),
            },
        )
        .await
        .context("failed to create observer service")?;

        let initial_chain_head = observer
            .block_loader()
            .load_simple(BlockId::latest())
            .await
            .context("failed to get latest block")?;

        let router_query = RouterQuery::new(&config.ethereum.rpc, config.ethereum.router_address)
            .await
            .with_context(|| "failed to create router query")?;

        let genesis_block_hash = router_query
            .genesis_block_hash()
            .await
            .with_context(|| "failed to query genesis hash")?;

        if genesis_block_hash.is_zero() {
            log::error!(
                "👶 Genesis block hash wasn't found. Call router.lookupGenesisHash() first"
            );

            bail!("Failed to query valid genesis hash");
        } else {
            log::info!("👶 Genesis block hash: {genesis_block_hash:?}");
        }

        let validators = router_query
            .validators_at(initial_chain_head.hash)
            .await
            .context("failed to query validators")?;
        log::info!("👥 Current validators set: {validators:?}");

        let threshold = router_query
            .validators_threshold()
            .await
            .with_context(|| "failed to query validators threshold")?;
        log::info!("🔒 Multisig threshold: {threshold} / {}", validators.len());

        log::info!(
            "🔧 Amount of chunk processing threads for programs processing: {}",
            config.node.chunk_processing_threads
        );

        let signer = Signer::fs(config.node.key_path.clone())?;

        let validator_pub_key = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?;

        // TODO #4642: use validator session key
        let _validator_pub_key_session =
            Self::get_config_public_key(config.node.validator_session, &signer)
                .with_context(|| "failed to get validator session private key")?;

        let consensus: Option<Pin<Box<dyn ConsensusService>>> = if let Some(pub_key) =
            validator_pub_key
        {
            let ethereum = EthereumBuilder::default()
                .rpc_url(&config.ethereum.rpc)
                .router_address(config.ethereum.router_address)
                .signer(signer.clone())
                .sender_address(pub_key.to_address())
                .eip1559_fee_increase_percentage(config.ethereum.eip1559_fee_increase_percentage)
                .eip1559_max_fee_per_gas_in_gwei(config.ethereum.eip1559_max_fee_per_gas_in_gwei)
                .blob_gas_multiplier(config.ethereum.blob_gas_multiplier)
                .build()
                .await?;
            Some(Box::pin(ValidatorService::new(
                signer.clone(),
                ethereum.middleware().query(),
                ethereum.router(),
                db.clone(),
                ValidatorConfig {
                    pub_key,
                    signatures_threshold: threshold,
                    // Coordinator-local: not a protocol constant; configured per node.
                    commitment_delay_limit: config.node.commitment_delay_limit,
                    router_address: config.ethereum.router_address,
                    batch_size_limit: config.node.batch_size_limit,
                    coordinator_aggregation_delay: config.node.coordinator_aggregation_delay,
                    uncommitted_chain_len_threshold: config.node.uncommitted_chain_len_threshold,
                },
            )?))
        } else {
            None
        };

        let network = if let Some(net_config) = &config.network {
            let network_signer = match net_config.transport_type {
                TransportType::Test => {
                    let network_signer = Signer::memory();
                    let network_private_key = signer
                        .private_key(net_config.public_key)
                        .with_context(|| "failed to get test network private key")?
                        .clone();
                    network_signer.import(network_private_key)?;
                    network_signer
                }
                TransportType::Default => Signer::fs(
                    config
                        .node
                        .key_path
                        .parent()
                        .context("key_path has no parent directory")?
                        .join("net"),
                )?,
            };

            let runtime_config = NetworkRuntimeConfig {
                latest_block_header: initial_chain_head.header,
                latest_validators: validators.clone(),
                validator_key: validator_pub_key,
                general_signer: signer.clone(),
                network_signer,
                db: db.clone(),
            };

            let network = NetworkService::new(net_config.clone(), runtime_config)
                .with_context(|| "failed to create network service")?;
            Some(network)
        } else {
            None
        };

        // RPC subscribers need every promise; validators emit on consensus only.
        let promises_mode = if rpc.is_some() {
            PromiseEmissionMode::AlwaysEmit
        } else {
            PromiseEmissionMode::ConsensusDriven
        };
        let processor_config = ProcessorConfig {
            chunk_size: config.node.chunk_processing_threads,
        };
        let processor = Processor::with_config(processor_config, db.clone())?;
        let compute = ComputeService::with_promise_mode(db.clone(), processor, promises_mode);

        // Malachite consensus service.

        let malachite_starter = {
            let malachite_home = config
                .node
                .database_path_for(config.ethereum.router_address)
                .join("malachite");

            let malachite_validator_set = build_malachite_validator_set(
                validators.iter().copied(),
                &config.malachite.validator_pub_keys,
            )?;

            let malachite_config = MalachiteServiceConfig::from_home_dir(malachite_home)
                .with_listen_addr(config.malachite.listen_addr)
                .with_persistent_peers(config.malachite.persistent_peers.clone())
                .with_canonical_quarantine(config.node.canonical_quarantine)
                .with_post_quarantine_delay(config.node.post_quarantine_delay)
                .with_validators(malachite_validator_set);

            log::info!(
                "Malachite listen: {}  persistent_peers: {}",
                malachite_config.listen_addr,
                malachite_config.persistent_peers.len(),
            );

            let validator_config =
                validator_pub_key.map(|pub_key| ethexe_malachite::ValidatorConfig {
                    pub_key,
                    mempool: InjectedTxMempool::new(db.clone()),
                    signer: signer.clone(),
                });

            let role = validator_config
                .as_ref()
                .map(|_| "validator")
                .unwrap_or("full");
            log::info!("Malachite node role: {role}");

            MalachiteServiceStarter::new(
                malachite_config,
                validator_config,
                db.clone(),
                initial_chain_head,
            )
            .context("failed to create Malachite service starter")?
        };

        let fast_sync = config.node.fast_sync;

        #[allow(unreachable_code)]
        Ok(Self {
            db,
            network,
            observer,
            blob_loader,
            compute,
            consensus,
            malachite_starter,
            signer,
            prometheus,
            rpc,
            fast_sync,
            validator_pub_key,
            shutdown_rx: None,
            #[cfg(test)]
            sender: unreachable!(),
        })
    }

    fn get_config_public_key(key: ConfigPublicKey, signer: &Signer) -> Result<Option<PublicKey>> {
        match key {
            ConfigPublicKey::Enabled(key) => Ok(Some(key)),
            ConfigPublicKey::Random => Ok(Some(signer.generate()?)),
            ConfigPublicKey::Disabled => Ok(None),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_parts(
        db: Database,
        observer: ObserverService,
        blob_loader: Box<dyn BlobLoaderService>,
        compute: ComputeService,
        signer: Signer,
        consensus: Option<Pin<Box<dyn ConsensusService>>>,
        malachite_starter: MalachiteServiceStarter,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcServer>,
        sender: tests::utils::TestingEventSender,
        fast_sync: bool,
        validator_pub_key: Option<PublicKey>,
    ) -> Self {
        Self {
            db,
            observer,
            blob_loader,
            compute,
            consensus,
            malachite_starter,
            signer,
            network,
            prometheus,
            rpc,
            sender,
            fast_sync,
            validator_pub_key,
            shutdown_rx: None,
        }
    }

    /// Install a graceful-shutdown channel. The returned sender,
    /// when fired, breaks the run loop at the next yield and then
    /// awaits [`ethexe_malachite::MalachiteService::shutdown`] before `run` returns —
    /// freeing the RocksDB advisory lock and libp2p listener
    /// synchronously, which a plain `JoinHandle::abort` does not
    /// guarantee.
    pub fn install_shutdown_channel(&mut self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        self.shutdown_rx = Some(rx);
        tx
    }

    pub async fn run(mut self) -> Result<()> {
        if self.fast_sync {
            fast_sync::sync(&mut self).await?;
        }

        self.run_inner().await.inspect_err(|err| {
            log::error!("Service finished work with error: {err:?}");
        })
    }

    async fn run_inner(self) -> Result<()> {
        let Service {
            db,
            mut network,
            mut observer,
            mut blob_loader,
            mut compute,
            mut consensus,
            malachite_starter,
            signer,
            mut prometheus,
            rpc,
            fast_sync: _,
            validator_pub_key,
            mut shutdown_rx,
            #[cfg(test)]
            sender,
        } = self;

        let mut malachite = malachite_starter.start().await?;

        let (mut rpc_handle, mut rpc) = if let Some(rpc) = rpc {
            log::info!("🌐 Rpc server starting at: {}", rpc.port());

            let (rpc_run, rpc_receiver) = rpc.run_server().await?;

            (Some(tokio::spawn(rpc_run.stopped())), Some(rpc_receiver))
        } else {
            (None, None)
        };

        let mut roles = vec!["Observer".to_string()];
        if let Some(c) = consensus.as_ref() {
            roles.push(c.role());
        }
        log::info!("⚙️ Node service starting, roles: {roles:?}");

        #[cfg(test)]
        sender
            .send(tests::utils::TestingEvent::ServiceStarted)
            .await;

        let mut network_injected_txs: HashMap<_, PendingNetworkInjectedTx> = HashMap::new();

        loop {
            let event: Event = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = consensus.maybe_next_some() => event?.into(),
                event = malachite.select_next_some() => event?.into(),
                event = network.maybe_next_some() => event.into(),
                event = observer.select_next_some() => event?.into(),
                event = blob_loader.select_next_some() => event?.into(),
                event = rpc.maybe_next_some() => event.into(),
                event = prometheus.maybe_next_some() => event.into(),
                _ = rpc_handle.as_mut().maybe() => {
                    bail!("`RPCWorker` has terminated, shutting down...")
                }
                _ = async { shutdown_rx.as_mut().unwrap().await }, if shutdown_rx.is_some() => {
                    log::info!("Graceful shutdown requested");
                    break;
                }
            };

            log::trace!("Primary service produced event, start handling: {event:?}");

            #[cfg(test)]
            sender.send(tests::utils::TestingEvent::new(&event)).await;

            match event {
                Event::Observer(event) => match event {
                    ObserverEvent::Block(block) => {
                        tracing::info!("📦 receive a ethereum chain head: {block}",);

                        if let Some(c) = consensus.as_mut() {
                            c.receive_new_chain_head(block)?;
                        }

                        malachite.receive_new_eb(block).await;
                    }
                    ObserverEvent::BlockSynced(block_hash) => {
                        log::info!("Ethereum block synced: {block_hash}");

                        compute.prepare_block(block_hash);

                        if let Some(c) = consensus.as_mut() {
                            c.receive_synced_block(block_hash)?;
                        }

                        if let Some(network) = network.as_mut() {
                            network.set_chain_head(block_hash)?;
                        }

                        malachite.receive_eb_synced(block_hash).await;
                    }
                },
                Event::BlobLoader(event) => match event {
                    BlobLoaderEvent::BlobLoaded(code_and_id) => {
                        compute.process_code(code_and_id);
                    }
                },
                Event::Compute(event) => match event {
                    ComputeEvent::RequestLoadCodes(codes) => {
                        blob_loader.load_codes(codes)?;
                    }
                    ComputeEvent::BlockPrepared(block_hash) => {
                        if let Some(c) = consensus.as_mut() {
                            c.receive_prepared_block(block_hash)?;
                        }

                        malachite.receive_eb_prepared(block_hash).await;
                    }
                    ComputeEvent::CodeProcessed(_) => {
                        // Nothing
                    }
                    ComputeEvent::MbComputed(mb_hash) => {
                        tracing::info!(mb_hash = %mb_hash, "MB executed");
                        // Monotonic by height — predecessor recomputes
                        // (catch-up replay) must not retreat the RPC tip.
                        let new_height = db
                            .mb_compact_block(mb_hash)
                            .expect("MbComputed invariant: CompactMb persisted before emit")
                            .height;
                        db.globals_mutate(|g| {
                            let prev = g.latest_computed_mb_hash;
                            let advance = if prev.is_zero() {
                                true
                            } else {
                                let prev_height = db
                                    .mb_compact_block(prev)
                                    .expect("latest_computed_mb_hash always points at a stored MB")
                                    .height;
                                new_height >= prev_height
                            };
                            if advance {
                                g.latest_computed_mb_hash = mb_hash;
                            }
                        });

                        if let Some(rpc) = &rpc {
                            rpc.receive_mb_computed(mb_hash);
                        }
                    }
                    ComputeEvent::Promise(promise, _mb_hash) => {
                        // The local node always feeds its computed body
                        // into the RPC subscription manager so the
                        // matching producer signature (which arrives via
                        // gossip or local self-signing below) can be
                        // joined into a full SignedTxReceipt.
                        if let Some(rpc) = &rpc {
                            rpc.receive_computed_promise(promise.clone());
                        }

                        // Producers additionally sign the promise hash
                        // and gossip the compact form so other nodes can
                        // reconstruct the full SignedTxReceipt once they
                        // compute the matching body locally.
                        if let Some(pub_key) = validator_pub_key {
                            let receipt = Receipt::Promise(promise.to_compact());

                            match signer.signed_message(pub_key, receipt, None) {
                                Ok(compact_receipt) => {
                                    if let Some(rpc) = rpc.as_ref() {
                                        rpc.receive_tx_receipt(compact_receipt.clone().into());
                                    }

                                    if let Some(net) = network.as_mut() {
                                        net.publish_tx_receipt(compact_receipt.into());
                                    }
                                }
                                Err(err) => {
                                    log::warn!("failed to sign compact promise: {err}");
                                }
                            }
                        }
                    }
                },
                Event::Network(event) => {
                    let Some(_) = network.as_mut() else {
                        unreachable!("couldn't produce event without network");
                    };

                    match event {
                        NetworkEvent::ValidatorMessage(message) => match message {
                            VerifiedValidatorMessage::RequestBatchValidation(request) => {
                                if let Some(c) = consensus.as_mut() {
                                    let request = request.map(|r| r.payload);
                                    c.receive_validation_request(request)?;
                                }
                            }
                            VerifiedValidatorMessage::ApproveBatch(reply) => {
                                if let Some(c) = consensus.as_mut() {
                                    let reply = reply.map(|r| r.payload);
                                    let (reply, _) = reply.into_parts();
                                    c.receive_validation_reply(reply)?;
                                }
                            }
                        },
                        NetworkEvent::InjectedTransaction(event) => match event {
                            ethexe_network::NetworkInjectedEvent::InboundTransaction {
                                peer: _,
                                transaction,
                                channel,
                            } => {
                                let acceptance = malachite
                                    .receive_injected_transaction(*transaction)
                                    .await
                                    .into();
                                if let Err(err) = channel.send(acceptance) {
                                    tracing::error!(
                                        ?err,
                                        "failed to send injected transaction acceptance response"
                                    )
                                }
                            }
                            ethexe_network::NetworkInjectedEvent::OutboundAcceptance {
                                transaction_hash,
                                acceptance,
                            } => {
                                let final_acceptance = network_injected_txs
                                    .get_mut(&transaction_hash)
                                    .and_then(|pending| pending.record_response(acceptance));

                                if let Some(final_acceptance) = final_acceptance
                                    && let Some(pending) =
                                        network_injected_txs.remove(&transaction_hash)
                                {
                                    for sender in pending.into_response_senders() {
                                        let _res = sender.send(final_acceptance.clone());
                                    }
                                }
                            }
                        },
                        NetworkEvent::TxReceiptMessage(receipt) => {
                            if let Some(rpc) = &rpc {
                                rpc.receive_tx_receipt(receipt);
                            }
                        }
                        NetworkEvent::ValidatorIdentityUpdated(_)
                        | NetworkEvent::PeerBlocked(_)
                        | NetworkEvent::PeerConnected(_) => {}
                    }
                }
                Event::Rpc(event) => {
                    log::trace!("Received RPC event: {event:?}");

                    match event {
                        RpcEvent::InjectedTransaction {
                            transaction,
                            response_sender,
                        } => {
                            let status = malachite
                                .receive_injected_transaction(transaction.clone())
                                .await;
                            let local_acceptance = InjectedTransactionAcceptance::from(status);

                            match network.as_mut() {
                                Some(network) => match local_acceptance {
                                    acceptance @ InjectedTransactionAcceptance::Accept => {
                                        // local consensus handle transaction, no need to wait for other acceptances
                                        if let Err(err) =
                                            network.broadcast_injected_transaction(transaction)
                                        {
                                            tracing::warn!(
                                                "failed to broadcast locally accepted injected transaction: error={err:?}"
                                            );
                                        }
                                        if let Err(err) = response_sender.send(acceptance) {
                                            tracing::error!(
                                                ?err,
                                                "failed to send local acceptance to RPC service, RPC channel dropped"
                                            )
                                        }
                                    }
                                    _ => {
                                        // local malachite rejected the transaction, wait for other acceptances
                                        let tx_hash = transaction.data().to_hash();
                                        if let Some(pending) =
                                            network_injected_txs.get_mut(&tx_hash)
                                        {
                                            pending.add_response_sender(response_sender);
                                            continue;
                                        }

                                        match network.broadcast_injected_transaction(transaction) {
                                            Ok(pending_responses) => {
                                                let pending = PendingNetworkInjectedTx::new(
                                                    response_sender,
                                                    pending_responses,
                                                    Some(local_acceptance),
                                                );
                                                network_injected_txs.insert(tx_hash, pending);
                                            }
                                            Err(err) => {
                                                let acceptance =
                                                    InjectedTransactionAcceptance::Reject {
                                                        reason: err.to_string(),
                                                    };

                                                if let Err(err) = response_sender.send(acceptance) {
                                                    tracing::error!(
                                                        ?err,
                                                        "failed to send local acceptance to RPC service, RPC channel dropped"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                },
                                None => {
                                    // No network, send local_acceptance to RPC
                                    let _ = response_sender.send(local_acceptance);
                                }
                            }
                        }
                    }
                }
                Event::Consensus(event) => match event {
                    ConsensusEvent::PublishMessage(message) => {
                        let Some(network) = network.as_mut() else {
                            continue;
                        };

                        network.publish_message(message);
                    }
                    ConsensusEvent::CommitmentSubmitted(info) => {
                        log::info!("{info}");
                    }
                    ConsensusEvent::Warning(msg) => {
                        log::warn!("Consensus service warning: {msg}");
                    }
                },
                Event::Malachite(event) => match event {
                    MalachiteEvent::BlockProposal { height, mb_hash } => {
                        tracing::info!(
                            height,
                            mb_hash = %mb_hash,
                            "Malachite: BlockProposal",
                        );

                        compute.compute_mb(mb_hash, ethexe_common::PromisePolicy::Enabled);
                    }
                    MalachiteEvent::BlockFinalized {
                        cert,
                        height,
                        mb_hash,
                    } => {
                        tracing::info!(
                            height,
                            mb_hash = %mb_hash,
                            sigs = cert.signatures.len(),
                            "Malachite: BlockFinalized",
                        );

                        // No compute here: `BlockProposal` is always emitted
                        // before the matching `BlockFinalized` (on every node,
                        // including the sync path), so compute for this MB has
                        // already been triggered.
                    }
                    MalachiteEvent::PurgedTransactions {
                        eb_hash,
                        transactions,
                    } => {
                        tracing::trace!(
                            "purged {} transactions in ethereum block {eb_hash}",
                            transactions.len()
                        );
                        let Some(pub_key) = validator_pub_key else {
                            tracing::trace!(
                                "validator public key not found, can not sign purged transactions"
                            );
                            continue;
                        };

                        let Some(rpc) = rpc.as_ref() else {
                            tracing::trace!(
                                "can not produce receipts for purged transactions without RPC service"
                            );
                            continue;
                        };

                        transactions.into_iter().for_each(|purged_tx| {
                            let receipt = Receipt::<CompactPromise>::Purged(purged_tx);
                            match signer.signed_message(pub_key, receipt, None) {
                                Ok(signed_receipt) => rpc.receive_tx_receipt(signed_receipt.into()),
                                Err(err) => {
                                    tracing::error!(
                                        "failed to sign purged transaction receipt: {err}"
                                    );
                                }
                            }
                        });
                    }
                },
                Event::Prometheus(event) => match event {
                    PrometheusEvent::CollectMetrics { libp2p_metrics } => {
                        if let Some(network) = &network {
                            let mut s = String::new();
                            network.render_libp2p_metrics(&mut s);
                            let _res = libp2p_metrics.send(s);
                        }
                    }
                    PrometheusEvent::ServerClosed(result) => {
                        bail!("Prometheus server closed with result: {result:?}");
                    }
                },
            }
        }

        malachite.shutdown().await;

        Ok(())
    }
}

struct GenesisInitializerFromFile {
    state_path: PathBuf,
    processor: Processor,
}

impl GenesisInitializerFromFile {
    pub fn new(genesis_state_path: PathBuf) -> Result<Self> {
        // Safety: in context of GenesisInitializerFromFile, processor doesn't access the database,
        // it's only used for code processing, so it's safe to create it with an empty database.
        #[allow(unused_unsafe)]
        let db = unsafe { Database::memory() };
        let processor = Processor::new(db)?;

        Ok(Self {
            state_path: genesis_state_path,
            processor,
        })
    }
}

impl GenesisInitializer for GenesisInitializerFromFile {
    fn get_genesis_data(&mut self) -> anyhow::Result<ethexe_db::dump::StateDump> {
        StateDump::read_from_file(&self.state_path)
    }

    fn process_code(&mut self, code_id: CodeId, code: Vec<u8>) -> ethexe_db::CodeProcessingFuture {
        let mut cloned_processor = self.processor.clone();
        async move {
            let ProcessedCodeInfo {
                code_id: _,
                valid: info,
            } = cloned_processor
                .process_code(CodeAndIdUnchecked { code_id, code })
                .await?;

            let Some(ValidCodeInfo {
                code: _,
                instrumented_code,
                code_metadata,
            }) = info
            else {
                return Ok(None);
            };

            Ok(Some((instrumented_code, code_metadata)))
        }
        .boxed()
    }
}
