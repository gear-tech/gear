// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::config::{Config, ConfigPublicKey};
use alloy::{
    node_bindings::{Anvil, AnvilInstance},
    providers::{ProviderBuilder, RootProvider, ext::AnvilApi},
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ethexe_blob_loader::{BlobLoader, BlobLoaderEvent, BlobLoaderService, ConsensusLayerConfig};
use ethexe_common::{COMMITMENT_DELAY_LIMIT, gear::CodeState, network::VerifiedValidatorMessage};
use ethexe_compute::{ComputeConfig, ComputeEvent, ComputeService};
use ethexe_consensus::{
    ConnectService, ConsensusEvent, ConsensusService, ValidatorConfig, ValidatorService,
};
use ethexe_db::{Database, RocksDatabase};
use ethexe_ethereum::{Ethereum, deploy::EthereumDeployer, router::RouterQuery};
use ethexe_network::{
    NetworkEvent, NetworkRuntimeConfig, NetworkService,
    db_sync::{self, ExternalDataProvider},
};
use ethexe_observer::{
    ObserverEvent, ObserverService,
    utils::{BlockId, BlockLoader},
};
use ethexe_processor::{Processor, ProcessorConfig};
use ethexe_prometheus::{PrometheusEvent, PrometheusService};
use ethexe_rpc::{RpcEvent, RpcServer};
use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _};
use futures::{StreamExt, stream::FuturesUnordered};
use gprimitives::{ActorId, CodeId, H256};
use gsigner::secp256k1::{Address, PrivateKey, PublicKey, Signer};
use std::{
    collections::{BTreeSet, HashMap},
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
    Network(NetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Prometheus(PrometheusEvent),
    Rpc(RpcEvent),
    Fetching(db_sync::HandleResult),
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

/// ethexe service.
pub struct Service {
    db: Database,
    observer: ObserverService,
    blob_loader: Box<dyn BlobLoaderService>,
    compute: ComputeService,
    consensus: Pin<Box<dyn ConsensusService>>,
    signer: Signer,

    // Optional services
    network: Option<NetworkService>,
    prometheus: Option<PrometheusService>,
    rpc: Option<RpcServer>,

    fast_sync: bool,
    validator_address: Option<Address>,

    #[cfg(test)]
    sender: tests::utils::TestingEventSender,
}

impl Service {
    /// Number of reserved dev accounts (deployer, validator).
    const RESERVED_DEV_ACCOUNTS: u32 = 2;

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
                .with_generated_verifiable_secret_sharing_commitment()
                .deploy()
                .await?;

        let provider: RootProvider = ProviderBuilder::default()
            .connect(anvil.ws_endpoint().as_str())
            .await?;

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

        wvara.mint(validator_address, amount).await?;

        for (_, sender_address) in it {
            provider
                .anvil_set_balance(sender_address.into(), balance)
                .await?;

            wvara.mint(sender_address, amount).await?;
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
        let db = Database::from_one(&rocks_db);

        let consensus_config = ConsensusLayerConfig {
            ethereum_rpc: config.ethereum.rpc.clone(),
            ethereum_beacon_rpc: config.ethereum.beacon_rpc.clone(),
            beacon_block_time: alloy::eips::merge::SLOT_DURATION,
            attempts: const { NonZero::<u8>::new(3).unwrap() },
        };
        let blob_loader = BlobLoader::new(db.clone(), consensus_config)
            .await
            .context("failed to create blob loader")?
            .into_box();

        let observer =
            ObserverService::new(&config.ethereum, config.node.eth_max_sync_depth, db.clone())
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

        let processor = Processor::with_config(
            ProcessorConfig {
                chunk_processing_threads: config.node.chunk_processing_threads,
            },
            db.clone(),
        )
        .with_context(|| "failed to create processor")?;

        log::info!(
            "🔧 Amount of chunk processing threads for programs processing: {}",
            processor.config().chunk_processing_threads
        );

        let signer = Signer::fs(config.node.key_path.clone())?;

        let validator_pub_key = Self::get_config_public_key(config.node.validator, &signer)
            .with_context(|| "failed to get validator private key")?;
        let validator_address = validator_pub_key.map(|key| key.to_address());

        // TODO #4642: use validator session key
        let _validator_pub_key_session =
            Self::get_config_public_key(config.node.validator_session, &signer)
                .with_context(|| "failed to get validator session private key")?;

        let consensus: Pin<Box<dyn ConsensusService>> = {
            if let Some(pub_key) = validator_pub_key {
                let ethereum = Ethereum::new(
                    &config.ethereum.rpc,
                    config.ethereum.router_address,
                    signer.clone(),
                    pub_key.to_address(),
                )
                .await?;
                Box::pin(ValidatorService::new(
                    signer.clone(),
                    ethereum.middleware().query(),
                    ethereum.router(),
                    db.clone(),
                    ValidatorConfig {
                        pub_key,
                        signatures_threshold: threshold,
                        slot_duration: config.ethereum.block_time,
                        block_gas_limit: config.node.block_gas_limit,
                        // TODO: #4942 commitment_delay_limit is a protocol specific constant
                        // which better to be configurable by router contract
                        commitment_delay_limit: COMMITMENT_DELAY_LIMIT,
                        producer_delay: Duration::ZERO,
                        router_address: config.ethereum.router_address,
                        chain_deepness_threshold: config.node.chain_deepness_threshold,
                    },
                )?)
            } else {
                Box::pin(ConnectService::new(
                    db.clone(),
                    config.ethereum.block_time,
                    3,
                ))
            }
        };

        let prometheus = if let Some(config) = config.prometheus.clone() {
            Some(PrometheusService::new(config)?)
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
                latest_validators: validators,
                validator_key: validator_pub_key,
                general_signer: signer.clone(),
                network_signer,
                external_data_provider: Box::new(RouterDataProvider(router_query)),
                db: Box::new(db.clone()),
            };

            let network = NetworkService::new(net_config.clone(), runtime_config)
                .with_context(|| "failed to create network service")?;
            Some(network)
        } else {
            None
        };

        let rpc = config
            .rpc
            .as_ref()
            .map(|config| RpcServer::new(config.clone(), db.clone()));

        let compute_config = ComputeConfig::new(config.node.canonical_quarantine);
        let compute = ComputeService::new(compute_config, db.clone(), processor);

        let fast_sync = config.node.fast_sync;

        #[allow(unreachable_code)]
        Ok(Self {
            db,
            network,
            observer,
            blob_loader,
            compute,
            consensus,
            signer,
            prometheus,
            rpc,
            fast_sync,
            validator_address,
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
        consensus: Pin<Box<dyn ConsensusService>>,
        network: Option<NetworkService>,
        prometheus: Option<PrometheusService>,
        rpc: Option<RpcServer>,
        sender: tests::utils::TestingEventSender,
        fast_sync: bool,
        validator_address: Option<Address>,
    ) -> Self {
        Self {
            db,
            observer,
            blob_loader,
            compute,
            consensus,
            signer,
            network,
            prometheus,
            rpc,
            sender,
            fast_sync,
            validator_address,
        }
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
            db: _,
            mut network,
            mut observer,
            mut blob_loader,
            mut compute,
            mut consensus,
            signer: _signer,
            mut prometheus,
            rpc,
            fast_sync: _,
            validator_address,
            #[cfg(test)]
            sender,
        } = self;

        let (mut rpc_handle, mut rpc) = if let Some(rpc) = rpc {
            log::info!("🌐 Rpc server starting at: {}", rpc.port());

            let (rpc_run, rpc_receiver) = rpc.run_server().await?;

            (Some(tokio::spawn(rpc_run.stopped())), Some(rpc_receiver))
        } else {
            (None, None)
        };

        let roles = vec!["Observer".to_string(), consensus.role()];
        log::info!("⚙️ Node service starting, roles: {roles:?}");

        #[cfg(test)]
        sender
            .send(tests::utils::TestingEvent::ServiceStarted)
            .await;

        let mut network_fetcher = FuturesUnordered::new();
        let mut network_injected_txs: HashMap<_, oneshot::Sender<_>> = HashMap::new();

        loop {
            let event: Event = tokio::select! {
                event = compute.select_next_some() => event?.into(),
                event = consensus.select_next_some() => event?.into(),
                event = network.maybe_next_some() => event.into(),
                event = observer.select_next_some() => event?.into(),
                event = blob_loader.select_next_some() => event?.into(),
                event = prometheus.maybe_next_some() => event.into(),
                event = rpc.maybe_next_some() => event.into(),
                fetching_result = network_fetcher.maybe_next_some() => Event::Fetching(fetching_result),
                _ = rpc_handle.as_mut().maybe() => {
                    log::info!("`RPCWorker` has terminated, shutting down...");
                    continue;
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

                        consensus.receive_new_chain_head(block_data)?
                    }
                    ObserverEvent::BlockSynced(block) => {
                        // NOTE: Observer guarantees that, if `BlockSynced` event is emitted,
                        // then from latest synced block and up to `data.block_hash`:
                        // all blocks on-chain data (see OnChainStorage) is loaded and available in database.

                        compute.prepare_block(block);
                        consensus.receive_synced_block(block)?;
                        if let Some(network) = network.as_mut() {
                            network.set_chain_head(block)?;
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
                    ComputeEvent::AnnounceComputed(computed_data) => {
                        consensus.receive_computed_announce(computed_data)?
                    }
                    ComputeEvent::BlockPrepared(block_hash) => {
                        consensus.receive_prepared_block(block_hash)?
                    }
                    ComputeEvent::CodeProcessed(_) => {
                        // Nothing
                    }
                },
                Event::Network(event) => {
                    let Some(_) = network.as_mut() else {
                        unreachable!("couldn't produce event without network");
                    };

                    match event {
                        NetworkEvent::ValidatorMessage(message) => {
                            match message {
                                VerifiedValidatorMessage::Announce(announce) => {
                                    let announce = announce.map(|a| a.payload);
                                    consensus.receive_announce(announce)?
                                }
                                VerifiedValidatorMessage::RequestBatchValidation(request) => {
                                    let request = request.map(|r| r.payload);
                                    consensus.receive_validation_request(request)?
                                }
                                VerifiedValidatorMessage::ApproveBatch(reply) => {
                                    let reply = reply.map(|r| r.payload);
                                    let (reply, _) = reply.into_parts();
                                    consensus.receive_validation_reply(reply)?
                                }
                                _ => consensus.receive_verified_validator_message(message)?,
                            };
                        }
                        NetworkEvent::InjectedTransaction(event) => match event {
                            ethexe_network::NetworkInjectedEvent::InboundTransaction {
                                transaction,
                                channel,
                            } => {
                                let res = consensus.receive_injected_transaction(transaction);
                                channel
                                    .send(res.into())
                                    .expect("channel must never be closed");
                            }
                            ethexe_network::NetworkInjectedEvent::OutboundAcceptance {
                                transaction_hash,
                                acceptance,
                            } => {
                                let response_sender = network_injected_txs
                                    .remove(&transaction_hash)
                                    .expect("unknown transaction");
                                let _res = response_sender.send(acceptance);
                            }
                        },
                        NetworkEvent::PromiseMessage(promise) => {
                            if let Some(rpc) = &rpc {
                                rpc.provide_promise(promise);
                            }
                        }
                        NetworkEvent::ValidatorIdentityUpdated(_)
                        | NetworkEvent::PeerBlocked(_)
                        | NetworkEvent::PeerConnected(_) => {}
                    }
                }
                Event::Prometheus(event) => {
                    let Some(p) = prometheus.as_mut() else {
                        unreachable!("couldn't produce event without prometheus");
                    };

                    match event {
                        PrometheusEvent::CollectMetrics => {
                            let last_block = observer.last_block_number();
                            let pending_codes = blob_loader.pending_codes_len();

                            p.update_observer_metrics(last_block, pending_codes);

                            // Collect compute service metrics
                            let metrics = compute.get_metrics();

                            p.update_compute_metrics(
                                metrics.blocks_queue_len,
                                metrics.waiting_codes_count,
                                metrics.process_codes_count,
                                metrics
                                    .latest_committed_block
                                    .as_ref()
                                    .map(|b| b.header.height as u64),
                                metrics
                                    .latest_committed_block
                                    .as_ref()
                                    .map(|b| b.header.timestamp),
                                metrics.time_since_latest_committed_secs,
                            );

                            // TODO #4643: support metrics for consensus service
                        }
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
                                let acceptance = consensus
                                    .receive_injected_transaction(transaction.tx)
                                    .into();
                                let _res = response_sender.send(acceptance);
                            } else {
                                let Some(network) = network.as_mut() else {
                                    continue;
                                };

                                let tx_hash = transaction.tx.data().to_hash();

                                match network.send_injected_transaction(transaction) {
                                    Ok(()) => {
                                        network_injected_txs.insert(tx_hash, response_sender);
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
                    ConsensusEvent::ComputeAnnounce(announce) => compute.compute_announce(announce),
                    ConsensusEvent::PublishMessage(message) => {
                        let Some(network) = network.as_mut() else {
                            continue;
                        };

                        network.publish_message(message);
                    }
                    ConsensusEvent::BroadcastValidatorMessage(message) => {
                        if let Some(network) = network.as_mut() {
                            network.publish_message(message.clone());
                            consensus.receive_validator_message(message)?;
                        } else {
                            consensus.receive_validator_message(message)?;
                        }
                    }
                    ConsensusEvent::CommitmentSubmitted(info) => {
                        log::info!("{info}");
                    }
                    ConsensusEvent::Warning(msg) => {
                        log::warn!("Consensus service warning: {msg}");
                    }
                    ConsensusEvent::RequestAnnounces(request) => {
                        let Some(network) = network.as_mut() else {
                            panic!("Requesting announces is not allowed without network service");
                        };

                        network_fetcher.push(network.db_sync_handle().request(request.into()));
                    }
                    ConsensusEvent::AnnounceAccepted(_) | ConsensusEvent::AnnounceRejected(_) => {
                        // TODO #4940: consider to publish network message
                    }
                    ConsensusEvent::Promises(promises) => {
                        if rpc.is_none() && network.is_none() {
                            panic!("Promise without network or rpc");
                        }

                        if let Some(rpc) = &rpc {
                            rpc.provide_promises(promises.clone());
                        }

                        if let Some(network) = &mut network {
                            for promise in promises {
                                network.publish_promise(promise);
                            }
                        }
                    }
                },
                Event::Fetching(result) => {
                    let Some(network) = network.as_mut() else {
                        unreachable!("Fetching event is impossible without network service");
                    };

                    match result {
                        Ok(db_sync::Response::Announces(response)) => {
                            consensus.receive_announces_response(response)?;
                        }
                        Ok(resp) => {
                            panic!("only announces are requested currently, but got: {resp:?}");
                        }
                        Err((err, request)) => {
                            log::trace!(
                                "Retry fetching external data for request {request:?} due to error: {err:?}"
                            );
                            network_fetcher.push(network.db_sync_handle().retry(request));
                        }
                    }
                }
            }
        }
    }
}
