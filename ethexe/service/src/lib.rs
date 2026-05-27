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

use crate::config::{Config, ConfigPublicKey};
use alloy::{
    node_bindings::{Anvil, AnvilInstance},
    providers::{ProviderBuilder, RootProvider, ext::AnvilApi},
    rpc::types::anvil::Metadata,
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ethexe_blob_loader::{BlobLoader, BlobLoaderEvent, BlobLoaderService, ConsensusLayerConfig};
use ethexe_common::{
    CodeAndIdUnchecked, PromiseEmissionMode,
    db::{GlobalsStorageRW, MbStorageRO, OnChainStorageRO},
    gear::CodeState,
    injected::{CompactPromise, Receipt},
    network::VerifiedValidatorMessage,
};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_consensus::{ConsensusEvent, ConsensusService, ValidatorConfig, ValidatorService};
use ethexe_db::{
    Database, GenesisInitializer, InitConfig, RawDatabase, RocksDatabase, dump::StateDump,
};
use ethexe_ethereum::{EthereumBuilder, deploy::EthereumDeployer, router::RouterQuery};
use ethexe_malachite::{
    InjectedTxMempool, MalachiteConfig, MalachiteEvent, MalachiteService, ValidatorEntry,
};
use ethexe_network::{
    NetworkEvent, NetworkRuntimeConfig, NetworkService, db_sync::ExternalDataProvider,
};
use ethexe_observer::{
    ObserverConfig, ObserverEvent, ObserverService,
    utils::{BlockId, BlockLoader},
};
use ethexe_processor::{ProcessedCodeInfo, Processor, ProcessorConfig, ValidCodeInfo};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcServer};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use futures::{FutureExt, StreamExt};
use gprimitives::{ActorId, CodeId, H256};
use gsigner::secp256k1::{Address, PrivateKey, PublicKey, Secp256k1SignerExt, Signer};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    num::NonZero,
    path::PathBuf,
    pin::Pin,
    time::Duration,
};
use tokio::sync::oneshot;

pub mod config;

mod fast_sync;
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

#[derive(Clone)]
struct RouterDataProvider(RouterQuery);

#[async_trait]
impl ExternalDataProvider for RouterDataProvider {
    fn clone_boxed(&self) -> Box<dyn ExternalDataProvider> {
        Box::new(self.clone())
    }

    async fn programs_code_ids_at(
        self: Box<Self>,
        program_ids: BTreeSet<ActorId>,
        block: H256,
    ) -> Result<Vec<CodeId>> {
        self.0.programs_code_ids_at(program_ids, block).await
    }

    async fn codes_states_at(
        self: Box<Self>,
        code_ids: BTreeSet<CodeId>,
        block: H256,
    ) -> Result<Vec<CodeState>> {
        self.0.codes_states_at(code_ids, block).await
    }
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
    malachite: Option<MalachiteService>,
    signer: Signer,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcServer>,

    fast_sync: bool,
    validator_address: Option<Address>,
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

        let latest_block = observer
            .block_loader()
            .load_simple(BlockId::Latest)
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
            .validators_at(latest_block.hash)
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
        let validator_address = validator_pub_key.map(|key| key.to_address());

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
            // TODO: #4918 create Signer object correctly for test/prod environments
            let network_signer = Signer::fs(
                config
                    .node
                    .key_path
                    .parent()
                    .context("key_path has no parent directory")?
                    .join("net"),
            )?;

            let latest_block_data = observer
                .block_loader()
                .load_simple(BlockId::Latest)
                .await
                .context("failed to get latest block")?;

            let runtime_config = NetworkRuntimeConfig {
                latest_block_header: latest_block_data.header,
                latest_validators: validators.clone(),
                validator_key: validator_pub_key,
                general_signer: signer.clone(),
                network_signer,
                external_data_provider: Box::new(RouterDataProvider(router_query)),
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
        let malachite_home = config
            .node
            .database_path_for(config.ethereum.router_address)
            .join("malachite");
        let mut malachite_base_config = MalachiteConfig::from_home_dir(malachite_home)
            .with_listen_addr(config.malachite.listen_addr)
            .with_persistent_peers(config.malachite.persistent_peers.clone());
        // Must match the compute layer's quarantine or consensus deadlocks.
        malachite_base_config.canonical_quarantine = config.node.canonical_quarantine;
        malachite_base_config.post_quarantine_delay = config.node.post_quarantine_delay;
        log::info!(
            "Malachite listen: {}  persistent_peers: {}",
            malachite_base_config.listen_addr,
            malachite_base_config.persistent_peers.len(),
        );
        let malachite = {
            let malachite_validator_set = build_malachite_validator_set(
                validators.iter().copied(),
                &config.malachite.validator_pub_keys,
            )?;
            log::info!(
                "Malachite validators: {} (local role: {})",
                malachite_validator_set.len(),
                if validator_pub_key.is_some() {
                    "validator"
                } else {
                    "full"
                },
            );
            let malachite_config = malachite_base_config.with_validators(malachite_validator_set);
            Some(
                MalachiteService::new(
                    malachite_config,
                    db.clone(),
                    signer.clone(),
                    validator_pub_key,
                    std::sync::Arc::new(InjectedTxMempool::new(db.clone())),
                )
                .await
                .context("failed to start Malachite service")?,
            )
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
            malachite,
            signer,
            prometheus,
            rpc,
            fast_sync,
            validator_address,
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
        malachite: Option<MalachiteService>,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcServer>,
        sender: tests::utils::TestingEventSender,
        fast_sync: bool,
        validator_address: Option<Address>,
        validator_pub_key: Option<PublicKey>,
    ) -> Self {
        Self {
            db,
            observer,
            blob_loader,
            compute,
            consensus,
            malachite,
            signer,
            network,
            prometheus,
            rpc,
            sender,
            fast_sync,
            validator_address,
            validator_pub_key,
            shutdown_rx: None,
        }
    }

    /// Install a graceful-shutdown channel. The returned sender,
    /// when fired, breaks the run loop at the next yield and then
    /// awaits [`MalachiteService::shutdown`] before `run` returns —
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
            mut malachite,
            signer,
            mut prometheus,
            rpc,
            fast_sync: _,
            validator_address,
            validator_pub_key,
            shutdown_rx,
            #[cfg(test)]
            sender,
        } = self;
        let mut shutdown_rx = shutdown_rx;

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

        // One fan-out can park N senders under the same tx_hash (one per
        // recipient validator), so we hold a Vec — earlier inserts must
        // not be clobbered by later ones.
        let mut network_injected_txs: HashMap<_, Vec<oneshot::Sender<_>>> = HashMap::new();

        loop {
            let event: Event = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = consensus.maybe_next_some() => event?.into(),
                event = malachite.maybe_next_some() => event?.into(),
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
                    ObserverEvent::Block(block_data) => {
                        tracing::info!(
                            height = %block_data.header.height,
                            timestamp = %block_data.header.timestamp,
                            hash = %block_data.hash,
                            parent_hash = %block_data.header.parent_hash,
                            "📦 receive a chain head",
                        );
                        if let Some(c) = consensus.as_mut() {
                            c.receive_new_chain_head(block_data)?;
                        }
                    }
                    ObserverEvent::BlockSynced(block) => {
                        log::info!(
                            "Block synced: {}",
                            db.block_simple_data(block)
                                .context("Cannot find header of synced block")?
                        );

                        compute.prepare_block(block);
                        if let Some(c) = consensus.as_mut() {
                            c.receive_synced_block(block)?;
                        }
                        if let Some(network) = network.as_mut() {
                            network.set_chain_head(block)?;
                        }
                        if let Some(m) = malachite.as_mut() {
                            let block = db
                                .block_simple_data(block)
                                .context("Cannot find header of synced block")?;
                            m.receive_new_chain_head(block);
                        }
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
                        // Malachite's BlockProposal events are gated
                        // on the EB they advance over being prepared
                        // (so downstream compute_mb doesn't race the
                        // code-validation pipeline). Wake the gate
                        // here so pending events get drained.
                        if let Some(m) = malachite.as_ref() {
                            m.receive_eb_prepared(block_hash);
                        }
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
                                let acceptance = if let Some(m) = malachite.as_mut() {
                                    ethexe_malachite::classify_insert_outcome(
                                        m.receive_injected_transaction((*transaction).clone()),
                                    )
                                } else {
                                    ethexe_common::injected::InjectedTransactionAcceptance::Accept
                                };
                                let _ = channel.send(acceptance);
                            }
                            ethexe_network::NetworkInjectedEvent::OutboundAcceptance {
                                transaction_hash,
                                acceptance,
                            } => {
                                if let Some(senders) =
                                    network_injected_txs.get_mut(&transaction_hash)
                                    && let Some(response_sender) = senders.pop()
                                {
                                    let _res = response_sender.send(acceptance);
                                    if senders.is_empty() {
                                        network_injected_txs.remove(&transaction_hash);
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
                            // zero address means that no matter what validator will insert this tx.
                            let is_zero_address = transaction.recipient == Address::default();
                            let is_our_address = Some(transaction.recipient) == validator_address;

                            if is_zero_address || is_our_address {
                                let acceptance = if let Some(m) = malachite.as_mut() {
                                    ethexe_malachite::classify_insert_outcome(
                                        m.receive_injected_transaction(transaction.tx.clone()),
                                    )
                                } else {
                                    ethexe_common::injected::InjectedTransactionAcceptance::Accept
                                };
                                let _res = response_sender.send(acceptance);
                            } else {
                                let Some(network) = network.as_mut() else {
                                    continue;
                                };

                                let tx_hash = transaction.tx.data().to_hash();

                                match network.send_injected_transaction(transaction) {
                                    Ok(()) => {
                                        network_injected_txs
                                            .entry(tx_hash)
                                            .or_default()
                                            .push(response_sender);
                                    }
                                    Err(err) => {
                                        let _res = response_sender.send(Err(err).into());
                                    }
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
                        // Validators are interested in this MB's
                        // promises so they can gossip them; the
                        // service's `PromiseEmissionMode` can still
                        // force the policy to `Enabled` regardless.
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
                        // Non-proposer nodes (validators that didn't propose
                        // this height + every full/RPC node) first see the MB
                        // here. Trigger compute so the body — including any
                        // injected-tx `Promise` — is produced locally; the
                        // matching `SignedCompactPromise` arrives via the
                        // network and is joined into a full `SignedTxReceipt`
                        // by the RPC subscription manager. Calls are
                        // idempotent: a proposer that already computed via
                        // `BlockProposal` short-circuits on
                        // `mb_meta.computed`.
                        compute.compute_mb(mb_hash, ethexe_common::PromisePolicy::Enabled);
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

        // Graceful tear-down: hand the malachite engine a chance to
        // flush its WAL and release the RocksDB advisory lock and
        // libp2p listener. Without this, an immediate restart on
        // the same home directory races the previous lock release.
        if let Some(m) = malachite.take() {
            m.shutdown().await;
        }
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
