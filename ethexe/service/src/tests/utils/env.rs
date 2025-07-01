// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{
    tests::utils::{
        events::{
            ObserverEventsListener, ObserverEventsPublisher, ServiceEventsListener,
            TestingEventReceiver, TestingNetworkEvent,
        },
        TestingEvent,
    },
    Service,
};
use alloy::{
    eips::BlockId,
    node_bindings::{Anvil, AnvilInstance},
    providers::{ext::AnvilApi, Provider as _, RootProvider},
    rpc::types::{anvil::MineOptions, Header as RpcHeader},
};
use ethexe_blob_loader::{
    local::{LocalBlobLoader, LocalBlobStorage},
    BlobLoaderService,
};
use ethexe_common::{
    ecdsa::{PrivateKey, PublicKey},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    Address,
};
use ethexe_consensus::{ConsensusService, SimpleConnectService, ValidatorService};
use ethexe_db::Database;
use ethexe_ethereum::Ethereum;
use ethexe_network::{export::Multiaddr, NetworkConfig, NetworkService};
use ethexe_observer::{EthereumConfig, ObserverEvent, ObserverService};
use ethexe_processor::Processor;
use ethexe_rpc::{test_utils::RpcClient, RpcConfig, RpcService};
use ethexe_signer::Signer;
use ethexe_tx_pool::TxPoolService;
use futures::StreamExt;
use gear_core_errors::ReplyCode;
use gprimitives::{ActorId, CodeId, MessageId, H160, H256};
use rand::{prelude::StdRng, SeedableRng};
use roast_secp256k1_evm::frost::{
    keys,
    keys::{IdentifierList, PublicKeyPackage, VerifiableSecretSharingCommitment},
    Identifier, SigningKey,
};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{
    sync::{broadcast, broadcast::Sender},
    task,
    task::JoinHandle,
};
use tracing::Instrument;

/// Max network services which can be created by one test environment.
const MAX_NETWORK_SERVICES_PER_TEST: usize = 1000;

pub struct TestEnv {
    pub eth_cfg: EthereumConfig,
    #[allow(unused)]
    pub wallets: Wallets,
    pub blobs_storage: LocalBlobStorage,
    pub provider: RootProvider,
    pub ethereum: Ethereum,
    pub signer: Signer,
    pub validators: Vec<ValidatorConfig>,
    pub sender_id: ActorId,
    pub threshold: u64,
    pub block_time: Duration,
    pub continuous_block_generation: bool,

    /// In order to reduce amount of observers, we create only one observer and broadcast events to all subscribers.
    broadcaster: Sender<ObserverEvent>,
    db: Database,
    /// If network is enabled by test, then we store here:
    /// network service polling thread, bootstrap address and nonce for new node address generation.
    bootstrap_network: Option<(JoinHandle<()>, String, usize)>,

    _anvil: Option<AnvilInstance>,
    _events_stream: JoinHandle<()>,
}

impl TestEnv {
    pub async fn new(config: TestEnvConfig) -> anyhow::Result<Self> {
        let TestEnvConfig {
            validators,
            block_time,
            rpc,
            wallets,
            router_address,
            continuous_block_generation,
            network,
        } = config;

        log::info!(
            "📗 Starting new test environment. Continuous block generation: {continuous_block_generation}"
        );

        let (rpc_url, anvil) = match rpc {
            EnvRpcConfig::ProvidedURL(rpc_url) => {
                log::info!("📍 Using provided RPC URL: {rpc_url}");
                (rpc_url, None)
            }
            EnvRpcConfig::CustomAnvil {
                slots_in_epoch,
                genesis_timestamp,
            } => {
                let mut anvil = Anvil::new();

                if continuous_block_generation {
                    anvil = anvil.block_time(block_time.as_secs())
                }
                if let Some(slots_in_epoch) = slots_in_epoch {
                    anvil = anvil.arg(format!("--slots-in-an-epoch={slots_in_epoch}"));
                }
                if let Some(genesis_timestamp) = genesis_timestamp {
                    anvil = anvil.arg(format!("--timestamp={genesis_timestamp}"));
                }

                let anvil = anvil.spawn();

                log::info!("📍 Anvil started at {}", anvil.ws_endpoint());
                (anvil.ws_endpoint(), Some(anvil))
            }
        };

        let signer = Signer::memory();

        let mut wallets = if let Some(wallets) = wallets {
            Wallets::custom(&signer, wallets)
        } else {
            Wallets::anvil(&signer)
        };

        let validators: Vec<_> = match validators {
            ValidatorsConfig::PreDefined(amount) => (0..amount).map(|_| wallets.next()).collect(),
            ValidatorsConfig::Custom(keys) => keys
                .iter()
                .map(|k| {
                    let private_key = k.parse().unwrap();
                    signer.storage_mut().add_key(private_key).unwrap()
                })
                .collect(),
        };

        let (validators, verifiable_secret_sharing_commitment) =
            Self::define_session_keys(&signer, validators);

        let sender_address = wallets.next().to_address();

        let ethereum = if let Some(router_address) = router_address {
            log::info!("📗 Connecting to existing router at {router_address}");
            Ethereum::new(
                &rpc_url,
                router_address.parse().unwrap(),
                signer.clone(),
                sender_address,
            )
            .await?
        } else {
            log::info!("📗 Deploying new router");
            Ethereum::deploy(
                &rpc_url,
                validators
                    .iter()
                    .map(|k| k.public_key.to_address())
                    .collect(),
                signer.clone(),
                sender_address,
                verifiable_secret_sharing_commitment,
            )
            .await?
        };

        let router = ethereum.router();
        let router_query = router.query();
        let router_address = router.address();

        let db = Database::memory();

        let eth_cfg = EthereumConfig {
            rpc: rpc_url.clone(),
            beacon_rpc: Default::default(),
            router_address,
            block_time: config.block_time,
        };
        let mut observer = ObserverService::new(&eth_cfg, u32::MAX, db.clone())
            .await
            .unwrap();

        let blobs_storage = LocalBlobStorage::default();

        let provider = observer.provider().clone();

        let (broadcaster, _events_stream) = {
            let (sender, mut receiver) = broadcast::channel(2048);
            let cloned_sender = sender.clone();

            let (send_subscription_created, receive_subscription_created) =
                tokio::sync::oneshot::channel::<()>();

            let handle = task::spawn(
                async move {
                    send_subscription_created.send(()).unwrap();

                    while let Ok(event) = observer.select_next_some().await {
                        log::trace!(target: "test-event", "📗 Event: {event:?}");

                        cloned_sender
                            .send(event)
                            .inspect_err(|err| log::error!("Failed to broadcast event: {err}"))
                            .unwrap();

                        // At least one receiver is presented always, in order to avoid the channel dropping.
                        receiver
                            .recv()
                            .await
                            .inspect_err(|err| log::error!("Failed to receive event: {err}"))
                            .unwrap();
                    }

                    panic!("📗 Observer stream ended");
                }
                .instrument(tracing::trace_span!("observer-stream")),
            );
            receive_subscription_created.await.unwrap();

            (sender, handle)
        };

        let threshold = router_query.threshold().await?;

        let network_address = match network {
            EnvNetworkConfig::Disabled => None,
            EnvNetworkConfig::Enabled => Some(None),
            EnvNetworkConfig::EnabledWithCustomAddress(address) => Some(Some(address)),
        };

        let bootstrap_network = network_address.map(|maybe_address| {
            static NONCE: AtomicUsize = AtomicUsize::new(1);

            // mul MAX_NETWORK_SERVICES_PER_TEST to avoid address collision between different test-threads
            let nonce = NONCE.fetch_add(1, Ordering::SeqCst) * MAX_NETWORK_SERVICES_PER_TEST;
            let address = maybe_address.unwrap_or_else(|| format!("/memory/{nonce}"));

            let config_path = tempfile::tempdir().unwrap().keep();
            let multiaddr: Multiaddr = address.parse().unwrap();

            let mut config = NetworkConfig::new_test(config_path);
            config.listen_addresses = [multiaddr.clone()].into();
            config.external_addresses = [multiaddr.clone()].into();
            let mut service = NetworkService::new(config, &signer, db.clone()).unwrap();

            let local_peer_id = service.local_peer_id();

            let handle = task::spawn(
                async move {
                    loop {
                        let _event = service.select_next_some().await;
                    }
                }
                .instrument(tracing::trace_span!("network-stream")),
            );

            let bootstrap_address = format!("{address}/p2p/{local_peer_id}");

            (handle, bootstrap_address, nonce)
        });

        // By default, anvil set system time as block time. For testing purposes we need to have constant increment.
        if anvil.is_some() && !continuous_block_generation {
            provider
                .anvil_set_block_timestamp_interval(block_time.as_secs())
                .await
                .unwrap();
        }

        Ok(TestEnv {
            eth_cfg,
            wallets,
            provider,
            blobs_storage,
            ethereum,
            signer,
            validators,
            sender_id: ActorId::from(H160::from(sender_address.0)),
            threshold,
            block_time,
            continuous_block_generation,
            broadcaster,
            db,
            bootstrap_network,
            _anvil: anvil,
            _events_stream,
        })
    }

    pub fn new_node(&mut self, config: NodeConfig) -> Node {
        let NodeConfig {
            name,
            db,
            validator_config,
            rpc: service_rpc_config,
            fast_sync,
        } = config;

        let db = db.unwrap_or_else(Database::memory);

        let (network_address, network_bootstrap_address) = self
            .bootstrap_network
            .as_mut()
            .map(|(_, bootstrap_address, nonce)| {
                *nonce += 1;

                if *nonce % MAX_NETWORK_SERVICES_PER_TEST == 0 {
                    panic!("Too many network services created by one test env: max is {MAX_NETWORK_SERVICES_PER_TEST}");
                }

                (format!("/memory/{nonce}"), bootstrap_address.clone())
            })
            .unzip();

        Node {
            name,
            db,
            multiaddr: None,
            latest_fast_synced_block: None,
            eth_cfg: self.eth_cfg.clone(),
            receiver: None,
            blob_storage: self.blobs_storage.clone(),
            signer: self.signer.clone(),
            threshold: self.threshold,
            block_time: self.block_time,
            running_service_handle: None,
            validator_config,
            network_address,
            network_bootstrap_address,
            service_rpc_config,
            fast_sync,
        }
    }

    pub async fn upload_code(&self, code: &[u8]) -> anyhow::Result<WaitForUploadCode> {
        log::info!("📗 Upload code, len {}", code.len());

        let listener = self.observer_events_publisher().subscribe().await;

        // Lock the blob reader to lock any other threads that may use it
        let code_id = CodeId::generate(code);
        self.blobs_storage.add_code(code_id, code.to_vec()).await;

        let pending_builder = self
            .ethereum
            .router()
            .request_code_validation_with_sidecar(code)
            .await?;
        assert_eq!(pending_builder.code_id(), code_id);

        Ok(WaitForUploadCode { listener, code_id })
    }

    pub async fn create_program(
        &self,
        code_id: CodeId,
        initial_executable_balance: u128,
    ) -> anyhow::Result<WaitForProgramCreation> {
        log::info!("📗 Create program, code_id {code_id}");

        let listener = self.observer_events_publisher().subscribe().await;

        let router = self.ethereum.router();

        let (_, program_id) = router.create_program(code_id, H256::random()).await?;

        if initial_executable_balance != 0 {
            let program_address = program_id.to_address_lossy().0.into();
            router
                .wvara()
                .approve(program_address, initial_executable_balance)
                .await?;

            let mirror = self.ethereum.mirror(program_address.into_array().into());

            mirror
                .executable_balance_top_up(initial_executable_balance)
                .await?;
        }

        Ok(WaitForProgramCreation {
            listener,
            program_id,
        })
    }

    pub async fn send_message(
        &self,
        target: ActorId,
        payload: &[u8],
        value: u128,
    ) -> anyhow::Result<WaitForReplyTo> {
        log::info!("📗 Send message to {target}, payload len {}", payload.len());

        let listener = self.observer_events_publisher().subscribe().await;

        let program_address = Address::try_from(target)?;
        let program = self.ethereum.mirror(program_address);

        let (_, message_id) = program.send_message(payload, value).await?;

        Ok(WaitForReplyTo {
            listener,
            message_id,
        })
    }

    pub async fn approve_wvara(&self, program_id: ActorId) {
        log::info!("📗 Approving WVara for {program_id}");

        let program_address = Address::try_from(program_id).unwrap();
        let wvara = self.ethereum.router().wvara();
        wvara.approve_all(program_address.0.into()).await.unwrap();
    }

    pub async fn transfer_wvara(&self, program_id: ActorId, value: u128) {
        log::info!("📗 Transferring {value} WVara to {program_id}");

        let program_address = Address::try_from(program_id).unwrap();
        let wvara = self.ethereum.router().wvara();
        wvara
            .transfer(program_address.0.into(), value)
            .await
            .unwrap();
    }

    pub fn observer_events_publisher(&self) -> ObserverEventsPublisher {
        ObserverEventsPublisher {
            broadcaster: self.broadcaster.clone(),
            db: self.db.clone(),
        }
    }

    /// Force new block generation on rpc node.
    /// The difference between this method and `skip_blocks` is that
    /// `skip_blocks` will wait for the block event to be generated,
    /// while this method does not guarantee that.
    pub async fn force_new_block(&self) {
        if self.continuous_block_generation {
            // nothing to do: new block will be generated automatically
        } else {
            self.provider.evm_mine(None).await.unwrap();
        }
    }

    /// Force new `blocks_amount` blocks generation on rpc node,
    /// and wait for the block event to be generated.
    pub async fn skip_blocks(&self, blocks_amount: u32) {
        if self.continuous_block_generation {
            let mut blocks_count = 0;
            self.observer_events_publisher()
                .subscribe()
                .await
                .apply_until_block_event(|_| {
                    blocks_count += 1;
                    Ok((blocks_count >= blocks_amount).then_some(()))
                })
                .await
                .unwrap();
        } else {
            self.provider
                .evm_mine(Some(MineOptions::Options {
                    timestamp: None,
                    blocks: Some(blocks_amount.into()),
                }))
                .await
                .unwrap();
        }
    }

    /// Returns the index in validators list of the next block producer.
    ///
    /// ## Note
    /// This function is not completely thread-safe.
    /// If you have some other threads or processes,
    /// that can produce blocks for the same rpc node,
    /// then the return may be outdated.
    pub async fn next_block_producer_index(&self) -> usize {
        let timestamp = self.latest_block().await.timestamp;
        ethexe_consensus::block_producer_index(
            self.validators.len(),
            (timestamp + self.block_time.as_secs()) / self.block_time.as_secs(),
        )
    }

    pub async fn latest_block(&self) -> RpcHeader {
        self.provider
            .get_block(BlockId::latest())
            .await
            .unwrap()
            .expect("latest block always exist")
            .header
    }

    pub fn define_session_keys(
        signer: &Signer,
        validators: Vec<PublicKey>,
    ) -> (Vec<ValidatorConfig>, VerifiableSecretSharingCommitment) {
        let max_signers: u16 = validators.len().try_into().expect("conversion failed");
        let min_signers = max_signers
            .checked_mul(2)
            .expect("multiplication failed")
            .div_ceil(3);

        let maybe_validator_identifiers: anyhow::Result<Vec<_>, _> = validators
            .iter()
            .map(|public_key| {
                Identifier::deserialize(&ActorId::from(public_key.to_address()).into_bytes())
            })
            .collect();
        let validator_identifiers = maybe_validator_identifiers.expect("conversion failed");
        let identifiers = IdentifierList::Custom(&validator_identifiers);

        let mut rng = StdRng::seed_from_u64(123);

        let secret = SigningKey::deserialize(&[0x01; 32]).expect("conversion failed");

        let (secret_shares, public_key_package1) =
            keys::split(&secret, max_signers, min_signers, identifiers, &mut rng)
                .expect("key split failed");

        let verifiable_secret_sharing_commitment = secret_shares
            .values()
            .map(|secret_share| secret_share.commitment().clone())
            .next()
            .expect("conversion failed");

        let identifiers = validator_identifiers.clone().into_iter().collect();
        let public_key_package2 =
            PublicKeyPackage::from_commitment(&identifiers, &verifiable_secret_sharing_commitment)
                .expect("conversion failed");
        assert_eq!(public_key_package1, public_key_package2);

        (
            validators
                .into_iter()
                .zip(validator_identifiers.iter())
                .map(|(public_key, id)| {
                    let signing_share = *secret_shares[id].signing_share();
                    let private_key =
                        PrivateKey::from(<[u8; 32]>::try_from(signing_share.serialize()).unwrap());
                    ValidatorConfig {
                        public_key,
                        session_public_key: signer.storage_mut().add_key(private_key).unwrap(),
                    }
                })
                .collect(),
            verifiable_secret_sharing_commitment,
        )
    }
}

pub enum ValidatorsConfig {
    /// Take validator addresses from provided wallet, amount of validators is provided.
    PreDefined(usize),
    /// Custom validator eth-addresses in hex string format.
    #[allow(unused)]
    Custom(Vec<String>),
}

/// Configuration for the network service.
pub enum EnvNetworkConfig {
    /// Network service is disabled.
    Disabled,
    /// Network service is enabled. Network address will be generated.
    Enabled,
    #[allow(unused)]
    /// Network service is enabled. Network address is provided as String.
    EnabledWithCustomAddress(String),
}

pub enum EnvRpcConfig {
    #[allow(unused)]
    ProvidedURL(String),
    CustomAnvil {
        slots_in_epoch: Option<u64>,
        genesis_timestamp: Option<u64>,
    },
}

pub struct TestEnvConfig {
    /// How many validators will be in deployed router.
    /// By default uses 1 auto generated validator.
    pub validators: ValidatorsConfig,
    /// By default uses 1 second block time.
    pub block_time: Duration,
    /// By default creates new anvil instance if rpc is not provided.
    pub rpc: EnvRpcConfig,
    /// By default uses anvil hardcoded wallets if custom wallets are not provided.
    pub wallets: Option<Vec<String>>,
    /// If None (by default) new router will be deployed.
    /// In case of Some(_), will connect to existing router contract.
    pub router_address: Option<String>,
    /// Identify whether networks works (or have to works) in continuous block generation mode, false by default.
    pub continuous_block_generation: bool,
    /// Network service configuration, disabled by default.
    pub network: EnvNetworkConfig,
}

impl Default for TestEnvConfig {
    fn default() -> Self {
        Self {
            validators: ValidatorsConfig::PreDefined(1),
            block_time: Duration::from_secs(1),
            rpc: EnvRpcConfig::CustomAnvil {
                // speeds up block finalization, so we don't have to calculate
                // when the next finalized block is produced, which is convenient for tests
                slots_in_epoch: Some(1),
                // For deterministic tests we need to set fixed genesis timestamp
                genesis_timestamp: Some(1_000_000_000),
            },
            wallets: None,
            router_address: None,
            continuous_block_generation: false,
            network: EnvNetworkConfig::Disabled,
        }
    }
}

// TODO (breathx): consider to remove me in favor of crate::config::NodeConfig.
#[derive(Default)]
pub struct NodeConfig {
    /// Node name.
    pub name: Option<String>,
    /// Database, if not provided, will be created with MemDb.
    pub db: Option<Database>,
    /// Validator configuration, if provided then new node starts as validator.
    pub validator_config: Option<ValidatorConfig>,
    /// RPC configuration, if provided then new node starts with RPC service.
    pub rpc: Option<RpcConfig>,
    /// Do P2P database synchronization before the main loop
    pub fast_sync: bool,
}

impl NodeConfig {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    #[allow(unused)]
    pub fn db(mut self, db: Database) -> Self {
        self.db = Some(db);
        self
    }

    pub fn validator(mut self, config: ValidatorConfig) -> Self {
        self.validator_config = Some(config);
        self
    }

    pub fn service_rpc(mut self, rpc_port: u16) -> Self {
        let service_rpc_config = RpcConfig {
            listen_addr: SocketAddr::new("127.0.0.1".parse().unwrap(), rpc_port),
            cors: None,
            dev: false,
        };
        self.rpc = Some(service_rpc_config);

        self
    }

    pub fn fast_sync(mut self) -> Self {
        self.fast_sync = true;
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ValidatorConfig {
    /// Validator public key.
    pub public_key: PublicKey,
    /// Validator session public key.
    pub session_public_key: PublicKey,
}

/// Provides access to hardcoded anvil wallets or custom set wallets.
pub struct Wallets {
    wallets: Vec<PublicKey>,
    next_wallet: usize,
}

impl Wallets {
    pub fn anvil(signer: &Signer) -> Self {
        let accounts = vec![
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
            "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
            "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
            "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
            "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
            "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
            "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
            "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
        ];

        Self::custom(signer, accounts)
    }

    pub fn custom<S: AsRef<str>>(signer: &Signer, accounts: Vec<S>) -> Self {
        Self {
            wallets: accounts
                .into_iter()
                .map(|s| {
                    signer
                        .storage_mut()
                        .add_key(s.as_ref().parse().unwrap())
                        .unwrap()
                })
                .collect(),
            next_wallet: 0,
        }
    }

    pub fn next(&mut self) -> PublicKey {
        let pub_key = self.wallets.get(self.next_wallet).expect("No more wallets");
        self.next_wallet += 1;
        *pub_key
    }
}

pub struct Node {
    pub name: Option<String>,
    pub db: Database,
    pub multiaddr: Option<String>,
    pub latest_fast_synced_block: Option<H256>,

    eth_cfg: EthereumConfig,
    receiver: Option<TestingEventReceiver>,
    blob_storage: LocalBlobStorage,
    signer: Signer,
    threshold: u64,
    block_time: Duration,
    running_service_handle: Option<JoinHandle<()>>,
    validator_config: Option<ValidatorConfig>,
    network_address: Option<String>,
    network_bootstrap_address: Option<String>,
    service_rpc_config: Option<RpcConfig>,
    fast_sync: bool,
}

impl Node {
    pub async fn start_service(&mut self) {
        assert!(
            self.running_service_handle.is_none(),
            "Service is already running"
        );

        let processor = Processor::new(self.db.clone()).unwrap();

        let wait_for_network = self.network_bootstrap_address.is_some();

        let network = self.network_address.as_ref().map(|addr| {
            let config_path = tempfile::tempdir().unwrap().keep();
            let multiaddr: Multiaddr = addr.parse().unwrap();

            let mut config = NetworkConfig::new_test(config_path);
            config.listen_addresses = [multiaddr.clone()].into();
            config.external_addresses = [multiaddr.clone()].into();
            if let Some(bootstrap_addr) = self.network_bootstrap_address.as_ref() {
                let multiaddr = bootstrap_addr.parse().unwrap();
                config.bootstrap_addresses = [multiaddr].into();
            }
            let network = NetworkService::new(config, &self.signer, self.db.clone()).unwrap();
            self.multiaddr = Some(format!("{addr}/p2p/{}", network.local_peer_id()));
            network
        });

        let consensus: Pin<Box<dyn ConsensusService>> =
            if let Some(config) = self.validator_config.as_ref() {
                Box::pin(
                    ValidatorService::new(
                        self.signer.clone(),
                        self.db.clone(),
                        ethexe_consensus::ValidatorConfig {
                            ethereum_rpc: self.eth_cfg.rpc.clone(),
                            pub_key: config.public_key,
                            router_address: self.eth_cfg.router_address,
                            signatures_threshold: self.threshold,
                            slot_duration: self.block_time,
                        },
                    )
                    .await
                    .unwrap(),
                )
            } else {
                Box::pin(SimpleConnectService::new())
            };

        let (sender, receiver) = broadcast::channel(2048);

        let observer = ObserverService::new(&self.eth_cfg, u32::MAX, self.db.clone())
            .await
            .unwrap();

        let blob_loader =
            LocalBlobLoader::new(self.db.clone(), self.blob_storage.clone()).into_box();

        let tx_pool_service = TxPoolService::new(self.db.clone());

        let rpc = self.service_rpc_config.as_ref().map(|service_rpc_config| {
            RpcService::new(service_rpc_config.clone(), self.db.clone(), None)
        });

        self.receiver = Some(receiver);

        let service = Service::new_from_parts(
            self.db.clone(),
            observer,
            blob_loader,
            processor,
            self.signer.clone(),
            tx_pool_service,
            consensus,
            network,
            None,
            rpc,
            sender,
            self.fast_sync,
        );

        let name = self.name.clone();
        let handle = task::spawn(async move {
            service
                .run()
                .instrument(tracing::info_span!("node", name))
                .await
                .unwrap()
        });
        self.running_service_handle = Some(handle);

        if self.fast_sync {
            self.latest_fast_synced_block = self
                .listener()
                .apply_until(|e| {
                    if let TestingEvent::FastSyncDone(block) = e {
                        Ok(Some(block))
                    } else {
                        Ok(None)
                    }
                })
                .await
                .map(Some)
                .unwrap();
        }

        self.wait_for(|e| matches!(e, TestingEvent::ServiceStarted))
            .await;

        // fast sync implies network has connections
        if wait_for_network && !self.fast_sync {
            self.wait_for(|e| {
                matches!(
                    e,
                    TestingEvent::Network(TestingNetworkEvent::PeerConnected(_))
                )
            })
            .await;
        }
    }

    pub async fn stop_service(&mut self) {
        let handle = self
            .running_service_handle
            .take()
            .expect("Service is not running");
        handle.abort();

        assert!(handle.await.unwrap_err().is_cancelled());

        self.multiaddr = None;
        self.receiver = None;
    }

    pub fn rpc_client(&self) -> Option<RpcClient> {
        self.service_rpc_config
            .as_ref()
            .map(|rpc| RpcClient::new(format!("http://{}", rpc.listen_addr)))
    }

    pub fn listener(&mut self) -> ServiceEventsListener {
        ServiceEventsListener {
            receiver: self.receiver.as_mut().expect("channel isn't created"),
        }
    }

    // TODO(playX18): Tests that actually use Event broadcast channel extensively
    pub async fn wait_for(&mut self, f: impl Fn(TestingEvent) -> bool) {
        self.listener()
            .wait_for(|e| Ok(f(e)))
            .await
            .expect("infallible; always ok")
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(handle) = &self.running_service_handle {
            handle.abort();
        }
    }
}

#[derive(Clone)]
pub struct WaitForUploadCode {
    listener: ObserverEventsListener,
    pub code_id: CodeId,
}

#[derive(Debug)]
pub struct UploadCodeInfo {
    pub code_id: CodeId,
    pub valid: bool,
}

impl WaitForUploadCode {
    pub async fn wait_for(mut self) -> anyhow::Result<UploadCodeInfo> {
        log::info!("📗 Waiting for code upload, code_id {}", self.code_id);

        let mut valid_info = None;

        self.listener
            .apply_until_block_event(|event| match event {
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid })
                    if code_id == self.code_id =>
                {
                    valid_info = Some(valid);
                    Ok(Some(()))
                }
                _ => Ok(None),
            })
            .await?;

        Ok(UploadCodeInfo {
            code_id: self.code_id,
            valid: valid_info.expect("Valid must be set"),
        })
    }
}

#[derive(Clone)]
pub struct WaitForProgramCreation {
    listener: ObserverEventsListener,
    pub program_id: ActorId,
}

#[derive(Debug)]
pub struct ProgramCreationInfo {
    pub program_id: ActorId,
    pub code_id: CodeId,
}

impl WaitForProgramCreation {
    pub async fn wait_for(mut self) -> anyhow::Result<ProgramCreationInfo> {
        log::info!("📗 Waiting for program {} creation", self.program_id);

        let mut code_id_info = None;
        self.listener
            .apply_until_block_event(|event| {
                match event {
                    BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id })
                        if actor_id == self.program_id =>
                    {
                        code_id_info = Some(code_id);
                        return Ok(Some(()));
                    }

                    _ => {}
                }
                Ok(None)
            })
            .await?;

        let code_id = code_id_info.expect("Code ID must be set");
        Ok(ProgramCreationInfo {
            program_id: self.program_id,
            code_id,
        })
    }
}

#[derive(Clone)]
pub struct WaitForReplyTo {
    listener: ObserverEventsListener,
    pub message_id: MessageId,
}

#[derive(Debug)]
pub struct ReplyInfo {
    pub message_id: MessageId,
    pub program_id: ActorId,
    pub payload: Vec<u8>,
    pub code: ReplyCode,
    pub value: u128,
}

impl WaitForReplyTo {
    pub async fn wait_for(mut self) -> anyhow::Result<ReplyInfo> {
        log::info!("📗 Waiting for reply to message {}", self.message_id);

        let mut info = None;

        self.listener
            .apply_until_block_event(|event| match event {
                BlockEvent::Mirror {
                    actor_id,
                    event:
                        MirrorEvent::Reply {
                            reply_to,
                            payload,
                            reply_code,
                            value,
                        },
                } if reply_to == self.message_id => {
                    info = Some(ReplyInfo {
                        message_id: reply_to,
                        program_id: actor_id,
                        payload,
                        code: reply_code,
                        value,
                    });
                    Ok(Some(()))
                }
                _ => Ok(None),
            })
            .await?;

        Ok(info.expect("Reply info must be set"))
    }
}
