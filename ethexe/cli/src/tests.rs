// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Integration tests.

use crate::service::Service;
use alloy::{
    node_bindings::{Anvil, AnvilInstance},
    providers::{ext::AnvilApi, Provider},
    rpc::types::anvil::MineOptions,
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::CodesStorage, mirror::Event as MirrorEvent, router::Event as RouterEvent, BlockEvent,
};
use ethexe_db::{Database, MemDb};
use ethexe_ethereum::{router::RouterQuery, Ethereum};
use ethexe_observer::{Event, MockBlobReader, Observer, Query};
use ethexe_processor::Processor;
use ethexe_sequencer::Sequencer;
use ethexe_signer::Signer;
use ethexe_validator::Validator;
use futures::StreamExt;
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, MessageId, H160, H256};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
    task::{self, JoinHandle},
};

struct Listener {
    receiver: Receiver<Event>,
    _handle: JoinHandle<()>,
}

impl Listener {
    pub async fn new(mut observer: Observer) -> Self {
        let (sender, receiver) = mpsc::channel::<Event>(1024);

        let (send_subscription_created, receive_subscription_created) = oneshot::channel::<()>();
        let _handle = task::spawn(async move {
            let observer_events = observer.events();
            futures::pin_mut!(observer_events);

            send_subscription_created.send(()).unwrap();

            while let Some(event) = observer_events.next().await {
                sender.send(event).await.unwrap();
            }
        });
        receive_subscription_created.await.unwrap();

        Self { receiver, _handle }
    }

    pub async fn next_event(&mut self) -> Result<Event> {
        self.receiver.recv().await.ok_or(anyhow!("No more events"))
    }

    pub async fn apply_until<R: Sized>(
        &mut self,
        mut f: impl FnMut(Event) -> Result<Option<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;
            if let Some(res) = f(event)? {
                return Ok(res);
            }
        }
    }

    pub async fn apply_until_block_event<R: Sized>(
        &mut self,
        mut f: impl FnMut(BlockEvent) -> Result<Option<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;

            let Event::Block(block) = event else {
                continue;
            };

            for event in block.events {
                if let Some(res) = f(event)? {
                    return Ok(res);
                }
            }
        }
    }
}

struct TestEnvConfig {
    rpc_url: String,
    router_address: Option<ethexe_signer::Address>,
    blob_reader: Option<Arc<MockBlobReader>>,
    validator_private_key: Option<ethexe_signer::PrivateKey>,
    block_time: Duration,
}

impl Default for TestEnvConfig {
    fn default() -> Self {
        Self {
            rpc_url: "ws://localhost:8545".to_string(),
            router_address: None,
            blob_reader: None,
            validator_private_key: None,
            block_time: Duration::from_secs(1),
        }
    }
}

impl TestEnvConfig {
    pub fn rpc_url(mut self, rpc_url: String) -> Self {
        self.rpc_url = rpc_url.to_string();
        self
    }
}

struct TestEnv {
    db: Database,
    blob_reader: Arc<MockBlobReader>,
    observer: Observer,
    ethereum: Ethereum,
    query: Query,
    router_query: RouterQuery,
    signer: Signer,
    rpc_url: String,
    sequencer_public_key: ethexe_signer::PublicKey,
    validator_private_key: ethexe_signer::PrivateKey,
    validator_public_key: ethexe_signer::PublicKey,
    router_address: ethexe_signer::Address,
    sender_address: ActorId,
    block_time: Duration,
    running_service_handle: Option<JoinHandle<Result<()>>>,
}

struct AnvilWallets {
    wallets: Vec<ethexe_signer::PublicKey>,
    next_wallet: usize,
}

impl AnvilWallets {
    pub fn new(signer: &Signer) -> Self {
        let accounts = [
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
        ]
        .map(|s| signer.add_key(s.parse().unwrap()).unwrap());

        Self {
            wallets: accounts.to_vec(),
            next_wallet: 0,
        }
    }

    pub fn next(&mut self) -> ethexe_signer::PublicKey {
        let pub_key = self.wallets.get(self.next_wallet).expect("No more wallets");
        self.next_wallet += 1;
        pub_key.clone()
    }
}

struct Lol {
    _anvil: Option<AnvilInstance>,
    rpc_url: String,
    wallets: AnvilWallets,

    observer: Observer,
    blob_reader: Arc<MockBlobReader>,
    ethereum: Ethereum,
    router_query: RouterQuery,
    signer: Signer,
    validators: Vec<ethexe_signer::PublicKey>,
    router_address: ethexe_signer::Address,
    sender_id: ActorId,
    genesis_block_hash: H256,
    threshold: u64,
    block_time: Duration,
}

impl Lol {
    pub async fn new(validators_amount: usize) -> Result<Self> {
        let (rpc_url, anvil) = match std::env::var("__ETHEXE_CLI_TESTS_RPC_URL") {
            Ok(rpc_url) => {
                log::info!("📍 Using provided RPC URL: {}", rpc_url);
                (rpc_url, None)
            },
            Err(_) => {
                let mut anvil = Anvil::new().try_spawn().unwrap();
                drop(anvil.child_mut().stdout.take()); //temp fix for alloy#1078
                log::info!("📍 Anvil started at {}", anvil.ws_endpoint());
                (anvil.ws_endpoint(), Some(anvil))
            }
        };

        let signer = Signer::new(tempfile::tempdir()?.into_path())?;
        let mut wallets = AnvilWallets::new(&signer);

        let sender_address = wallets.next().to_address();
        let validators: Vec<_> = (0..validators_amount)
            .map(|_| signer.generate_key().unwrap())
            .collect();

        let ethereum = Ethereum::deploy(
            &rpc_url,
            validators.iter().map(|k| k.to_address()).collect(),
            signer.clone(),
            sender_address,
        )
        .await?;

        let router = ethereum.router();
        let router_query = router.query();
        let router_address = router.address();

        let block_time = Duration::from_secs(1);

        let blob_reader = Arc::new(MockBlobReader::new(block_time));

        let observer = Observer::new(&rpc_url, router_address, blob_reader.clone())
            .await
            .expect("failed to create observer");

        let genesis_block_hash = router_query.genesis_block_hash().await?;
        let threshold = router_query.threshold().await?;

        Ok(Lol {
            _anvil: anvil,
            rpc_url,
            wallets,
            observer,
            blob_reader,
            ethereum,
            router_query,
            signer,
            validators,
            router_address,
            sender_id: ActorId::from(H160::from(sender_address.0)),
            genesis_block_hash,
            threshold,
            block_time,
        })
    }

    fn create_node(&self, db: Option<Database>) -> Node {
        Node {
            db: db.unwrap_or_else(|| Database::from_one(&MemDb::default(), self.router_address.0)),
            rpc_url: self.rpc_url.clone(),
            genesis_block_hash: self.genesis_block_hash,
            blob_reader: self.blob_reader.clone(),
            observer: self.observer.clone(),
            signer: self.signer.clone(),
            block_time: self.block_time,
            validators: self.validators.iter().map(|k| k.to_address()).collect(),
            threshold: self.threshold,
            router_address: self.router_address,
            running_service_handle: None,
        }
    }

    pub async fn upload_code(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        log::info!("📗 Uploading code len {}", code.len());
        let code_id = CodeId::generate(code);
        let blob_tx = H256::random();

        self.blob_reader
            .add_blob_transaction(blob_tx, code.to_vec())
            .await;
        let tx_hash = self
            .ethereum
            .router()
            .request_code_validation(code_id, blob_tx)
            .await?;

        Ok((tx_hash, code_id))
    }

    pub async fn new_listener(&self) -> Listener {
        Listener::new(self.observer.clone()).await
    }
}

struct Node {
    db: Database,
    rpc_url: String,
    genesis_block_hash: H256,
    blob_reader: Arc<MockBlobReader>,
    observer: Observer,
    signer: Signer,
    validators: Vec<ethexe_signer::Address>,
    threshold: u64,
    router_address: ethexe_signer::Address,
    block_time: Duration,
    running_service_handle: Option<JoinHandle<Result<()>>>,
}

impl Node {
    pub async fn start_service(
        &mut self,
        sequencer_public_key: Option<ethexe_signer::PublicKey>,
        validator_public_key: Option<ethexe_signer::PublicKey>,
        network_address: Option<String>,
    ) {
        assert!(
            self.running_service_handle.is_none(),
            "Service is already running"
        );

        let processor = Processor::new(self.db.clone()).unwrap();

        let query = Query::new(
            Arc::new(self.db.clone()),
            &self.rpc_url,
            self.router_address,
            self.genesis_block_hash,
            self.blob_reader.clone(),
            10000,
        )
        .await
        .unwrap();

        let network = network_address.as_ref().map(|addr| {
            let config_path = tempfile::tempdir().unwrap().into_path();
            let config =
                ethexe_network::NetworkEventLoopConfig::new_memory(config_path, addr.as_str());
            ethexe_network::NetworkService::new(config, &self.signer).unwrap()
        });

        let sequencer = match sequencer_public_key.as_ref() {
            Some(key) => Some(
                Sequencer::new(
                    &ethexe_sequencer::Config {
                        ethereum_rpc: self.rpc_url.clone(),
                        sign_tx_public: *key,
                        router_address: self.router_address,
                        validators: self.validators.clone(),
                        threshold: self.threshold,
                    },
                    self.signer.clone(),
                )
                .await
                .unwrap(),
            ),
            None => None,
        };

        let validator = match validator_public_key.as_ref() {
            Some(key) => Some(Validator::new(
                &ethexe_validator::Config {
                    pub_key: *key,
                    router_address: self.router_address,
                },
                self.signer.clone(),
            )),
            None => None,
        };

        let service = Service::new_from_parts(
            self.db.clone(),
            self.observer.clone(),
            query,
            processor,
            self.signer.clone(),
            self.block_time,
            network,
            sequencer,
            validator,
            None,
            None,
        );

        let handle = task::spawn(service.run());
        self.running_service_handle = Some(handle);

        // Sleep to wait for the new service to start
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    pub async fn stop_service(&mut self) {
        let handle = self
            .running_service_handle
            .take()
            .expect("Service is not running");
        handle.abort();
        let _ = handle.await;
    }
}

impl TestEnv {
    async fn new(config: TestEnvConfig) -> Result<TestEnv> {
        let TestEnvConfig {
            rpc_url,
            router_address,
            blob_reader,
            validator_private_key,
            block_time,
        } = config;

        let tempdir = tempfile::tempdir()?.into_path();
        let signer = Signer::new(tempdir.join("key"))?;
        let sender_public_key = signer.add_key(
            // Anvil account (0) with balance
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?,
        )?;
        let (validator_private_key, validator_public_key) = match validator_private_key {
            Some(key) => (key, signer.add_key(key).unwrap()),
            None => {
                let pub_key = signer.generate_key()?;
                (signer.get_private_key(pub_key).unwrap(), pub_key)
            }
        };
        let sequencer_public_key = signer.add_key(
            // Anvil account (1) with balance
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".parse()?,
        )?;

        let sender_address = sender_public_key.to_address();
        let ethereum = if let Some(router_address) = router_address {
            Ethereum::new(&rpc_url, router_address, signer.clone(), sender_address).await?
        } else {
            let validators = vec![validator_public_key.to_address()];
            Ethereum::deploy(&rpc_url, validators, signer.clone(), sender_address).await?
        };

        let router = ethereum.router();
        let router_query = router.query();

        let genesis_block_hash = router_query.genesis_block_hash().await?;

        let blob_reader = blob_reader.unwrap_or_else(|| Arc::new(MockBlobReader::new(block_time)));

        let router_address = router.address();

        let db = Database::from_one(&MemDb::default(), router_address.0);

        let query = Query::new(
            Arc::new(db.clone()),
            &rpc_url,
            router_address,
            genesis_block_hash,
            blob_reader.clone(),
            10000,
        )
        .await?;

        let observer = Observer::new(&rpc_url, router_address, blob_reader.clone())
            .await
            .expect("failed to create observer");

        let env = TestEnv {
            db,
            query,
            blob_reader,
            observer,
            ethereum,
            router_query,
            signer,
            rpc_url,
            sequencer_public_key,
            validator_private_key,
            validator_public_key,
            router_address,
            sender_address: ActorId::from(H160::from(sender_address.0)),
            block_time,
            running_service_handle: None,
        };

        Ok(env)
    }

    pub fn start_anvil() -> AnvilInstance {
        let mut anvil = Anvil::new().try_spawn().unwrap();
        log::info!("📍 Anvil started at {}", anvil.ws_endpoint());
        drop(anvil.child_mut().stdout.take()); //temp fix for alloy#1078
        anvil
    }

    pub async fn new_listener(&self) -> Listener {
        Listener::new(self.observer.clone()).await
    }

    pub async fn start_service(&mut self) -> Result<()> {
        if self.running_service_handle.is_some() {
            return Err(anyhow!("Service is already running"));
        }

        let config_path = tempfile::tempdir()?.into_path();
        let config = ethexe_network::NetworkEventLoopConfig::new_memory(config_path, "/memory/1");
        let network = ethexe_network::NetworkService::new(config, &self.signer)?;

        let processor = Processor::new(self.db.clone())?;

        let sequencer = Sequencer::new(
            &ethexe_sequencer::Config {
                ethereum_rpc: self.rpc_url.clone(),
                sign_tx_public: self.sequencer_public_key,
                router_address: self.router_address,
                validators: vec![self.validator_public_key.to_address()],
                threshold: 1,
            },
            self.signer.clone(),
        )
        .await?;

        let validator = Validator::new(
            &ethexe_validator::Config {
                pub_key: self.validator_public_key,
                router_address: self.router_address,
            },
            self.signer.clone(),
        );

        let service = Service::new_from_parts(
            self.db.clone(),
            self.observer.clone(),
            self.query.clone(),
            processor,
            self.signer.clone(),
            self.block_time,
            Some(network),
            Some(sequencer),
            Some(validator),
            None,
            None,
        );

        let handle = task::spawn(service.run());
        self.running_service_handle = Some(handle);

        // Sleep to wait for the new service to start
        // TODO: find a better way to wait for the service to start #4099
        tokio::time::sleep(Duration::from_secs(1)).await;

        Ok(())
    }

    pub async fn stop_service(&mut self) -> Result<()> {
        if let Some(handle) = self.running_service_handle.take() {
            handle.abort();
            let _ = handle.await;
            Ok(())
        } else {
            Err(anyhow!("Service is not running"))
        }
    }

    pub async fn upload_code(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        let code_id = CodeId::generate(code);
        let blob_tx = H256::random();

        self.blob_reader
            .add_blob_transaction(blob_tx, code.to_vec())
            .await;
        let tx_hash = self
            .ethereum
            .router()
            .request_code_validation(code_id, blob_tx)
            .await?;

        Ok((tx_hash, code_id))
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(handle) = self.running_service_handle.take() {
            handle.abort();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping() {
    gear_utils::init_default_logger();

    let mut env = Lol::new(1).await.unwrap();
    let mut listener = env.new_listener().await;
    let mut node = env.create_node(None);

    node.start_service(Some(env.wallets.next()), Some(env.validators[0]), None)
        .await;

    let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

    log::info!("📗 Waiting for code loaded");
    listener
        .apply_until(|event| match event {
            Event::CodeLoaded {
                code_id: loaded_id,
                code,
            } => {
                assert_eq!(code_id, loaded_id);
                assert_eq!(&code, demo_ping::WASM_BINARY);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    log::info!("📗 Waiting for code to get validated");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::Router(RouterEvent::CodeGotValidated {
                id: loaded_id,
                valid,
            }) = event
            {
                assert_eq!(code_id, loaded_id);
                assert!(valid);
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    let code = node
        .db
        .original_code(code_id)
        .expect("After approval, the code is guaranteed to be in the database");
    let _ = node
        .db
        .instrumented_code(1, code_id)
        .expect("After approval, instrumented code is guaranteed to be in the database");
    assert_eq!(code, demo_ping::WASM_BINARY);

    let _ = env
        .ethereum
        .router()
        .create_program(code_id, H256::random(), b"PING", 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for program create, PONG reply and program update");

    let mut program_id = ActorId::default();
    let mut init_message_id = MessageId::default();
    let mut reply_sent = false;
    let mut block_committed = None;
    listener
        .apply_until_block_event(|event| {
            match event {
                BlockEvent::Router(RouterEvent::ProgramCreated {
                    actor_id,
                    code_id: loaded_id,
                }) => {
                    assert_eq!(code_id, loaded_id);
                    program_id = actor_id;
                }
                BlockEvent::Mirror { address, event } => {
                    if address == program_id {
                        match event {
                            MirrorEvent::MessageQueueingRequested {
                                id,
                                source,
                                payload,
                                value,
                            } => {
                                assert_eq!(source, env.sender_id);
                                assert_eq!(payload, b"PING");
                                assert_eq!(value, 0);
                                init_message_id = id;
                            }
                            MirrorEvent::Reply {
                                payload, reply_to, ..
                            } => {
                                assert_eq!(payload, b"PONG");
                                assert_eq!(reply_to, init_message_id);
                                reply_sent = true;
                            }
                            MirrorEvent::StateChanged { .. } => {
                                assert!(reply_sent);
                            }
                            _ => {}
                        }
                    }
                }
                BlockEvent::Router(RouterEvent::BlockCommitted { block_hash }) => {
                    block_committed = Some(block_hash);
                    return Ok(Some(()));
                }
                _ => {}
            }
            Ok(None)
        })
        .await
        .unwrap();

    let block_committed_on_router = env.router_query.last_commitment_block_hash().await.unwrap();
    assert_eq!(block_committed, Some(block_committed_on_router));

    let program_address = ethexe_signer::Address::try_from(program_id).unwrap();

    let wvara = env.ethereum.router().wvara();

    log::info!("📗 Approving WVara to mirror");
    wvara.approve_all(program_address.0.into()).await.unwrap();

    let ping_program = env.ethereum.mirror(program_address);

    log::info!("📗 Sending PING message");
    let _tx = ping_program.send_message(b"PING", 0).await.unwrap();

    log::info!("📗 Waiting for PONG reply");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror { address, event } => {
                if address == program_id {
                    if let MirrorEvent::Reply { payload, value, .. } = event {
                        assert_eq!(payload, b"PONG");
                        assert_eq!(value, 0);
                        return Ok(Some(()));
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        })
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_reorg() {
    gear_utils::init_default_logger();

    let mut _anvil = None;
    let rpc_url = if let Ok(lol) = std::env::var("LOL") {
        lol
    } else {
        let a = TestEnv::start_anvil();
        let url = a.ws_endpoint();
        _anvil = Some(a);
        url
    };

    let mut env = TestEnv::new(TestEnvConfig::default().rpc_url(rpc_url.clone()))
        .await
        .unwrap();
    let mut listener = env.new_listener().await;

    env.start_service().await.unwrap();

    let provider = env.observer.provider().clone();

    log::info!("📗 upload code");
    let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

    log::info!("📗 Waiting for code loaded");
    listener
        .apply_until(|event| match event {
            Event::CodeLoaded {
                code_id: loaded_id,
                code,
            } => {
                assert_eq!(code_id, loaded_id);
                assert_eq!(&code, demo_ping::WASM_BINARY);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    log::info!("📗 Waiting for code approval");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Router(RouterEvent::CodeGotValidated {
                id: validated_code_id,
                valid,
            }) => {
                assert_eq!(code_id, validated_code_id);
                assert!(valid);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    log::info!("📗 Abort service to simulate node blocks skipping");
    env.stop_service().await.unwrap();

    let _ = env
        .ethereum
        .router()
        .create_program(code_id, H256::random(), b"PING", 0)
        .await
        .unwrap();

    // Mine some blocks to check missed blocks support
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(10),
        }))
        .await
        .unwrap();

    // Start new service
    env.start_service().await.unwrap();

    // IMPORTANT: Mine one block to sent block event to the new service.
    provider.evm_mine(None).await.unwrap();

    log::info!("📗 Waiting for program creation");
    let mut program_id = ActorId::default();
    let mut init_message_id = MessageId::default();
    let mut reply_sent = false;
    listener
        .apply_until_block_event(|event| {
            match event {
                BlockEvent::Router(RouterEvent::ProgramCreated {
                    actor_id,
                    code_id: loaded_id,
                }) => {
                    assert_eq!(code_id, loaded_id);
                    program_id = actor_id;
                }
                BlockEvent::Mirror { address, event } => {
                    if address == program_id {
                        match event {
                            MirrorEvent::MessageQueueingRequested {
                                id,
                                source,
                                payload,
                                value,
                            } => {
                                assert_eq!(source, env.sender_address);
                                assert_eq!(payload, b"PING");
                                assert_eq!(value, 0);
                                init_message_id = id;
                            }
                            MirrorEvent::Reply {
                                payload, reply_to, ..
                            } => {
                                assert_eq!(payload, b"PONG");
                                assert_eq!(reply_to, init_message_id);
                                reply_sent = true;
                            }
                            MirrorEvent::StateChanged { .. } => {
                                assert!(reply_sent);
                            }
                            _ => {}
                        }
                    }
                }
                BlockEvent::Router(RouterEvent::BlockCommitted { .. }) => return Ok(Some(())),
                _ => {}
            };

            Ok(None)
        })
        .await
        .unwrap();

    let program_address = ethexe_signer::Address::try_from(program_id).unwrap();

    let wvara = env.ethereum.router().wvara();

    log::info!("📗 Approving WVara to mirror");
    wvara.approve_all(program_address.0.into()).await.unwrap();

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        provider.get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = provider.anvil_snapshot().await.unwrap();

    let ping_program = env.ethereum.mirror(program_address);

    log::info!("📗 Sending PING message");
    let _tx = ping_program.send_message(b"PING", 0).await.unwrap();

    log::info!("📗 Waiting for PONG reply");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror { address, event } => {
                if address == program_id {
                    if let MirrorEvent::Reply { payload, value, .. } = event {
                        assert_eq!(payload, b"PONG");
                        assert_eq!(value, 0);
                        return Ok(Some(()));
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    // Await for service block with user reply handling
    // TODO: this is for better logs reading only, should find a better solution #4099
    tokio::time::sleep(env.block_time).await;

    log::info!("📗 Reverting to the program creation snapshot");
    provider
        .anvil_revert(program_created_snapshot_id)
        .await
        .map(|res| assert!(res))
        .unwrap();

    log::info!("📗 Sending PING message after reorg");
    let _tx = ping_program.send_message(b"PING", 0).await.unwrap();

    log::info!("📗 Waiting for PONG reply after reorg");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror { address, event } => {
                if address == program_id {
                    if let MirrorEvent::Reply { payload, value, .. } = event {
                        assert_eq!(payload, b"PONG");
                        assert_eq!(value, 0);
                        return Ok(Some(()));
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    // The last step is to test correctness after db cleanup
    let router_address = env.router_address;
    let blob_reader = env.blob_reader.clone();
    let validator_private_key = env.validator_private_key;
    drop(env);

    log::info!("📗 Sending PING message, db cleanup and service shutting down");
    let _tx = ping_program.send_message(b"PING", 0).await.unwrap();

    // Skip some blocks to simulate long time without service
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(10),
        }))
        .await
        .unwrap();

    let mut env = TestEnv::new(TestEnvConfig {
        rpc_url,
        router_address: Some(router_address),
        blob_reader: Some(blob_reader),
        validator_private_key: Some(validator_private_key),
        ..Default::default()
    })
    .await
    .unwrap();
    env.start_service().await.unwrap();

    // Important: mine one block to sent block event to the new service.
    provider.evm_mine(None).await.unwrap();

    log::info!("📗 Waiting for PONG reply service restart on empty db");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror { address, event } => {
                if address == program_id {
                    if let MirrorEvent::Reply { payload, value, .. } = event {
                        assert_eq!(payload, b"PONG");
                        assert_eq!(value, 0);
                        return Ok(Some(()));
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    // Await for service block with user reply handling
    // TODO: this is for better logs reading only, should find a better solution #4099
    tokio::time::sleep(Duration::from_secs(1)).await;

    log::info!("📗 Done");
}

// Mine 150 blocks - send message - mine 150 blocks.
// Deep sync must load chain in batch.
#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_deep_sync() {
    gear_utils::init_default_logger();

    let anvil = TestEnv::start_anvil();

    let mut env = TestEnv::new(TestEnvConfig::default().rpc_url(anvil.ws_endpoint()))
        .await
        .unwrap();
    let mut listener = env.new_listener().await;

    env.start_service().await.unwrap();

    let provider = env.observer.provider().clone();

    let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

    log::info!("📗 Waiting for code loaded");
    listener
        .apply_until(|event| match event {
            Event::CodeLoaded {
                code_id: loaded_id,
                code,
            } => {
                assert_eq!(code_id, loaded_id);
                assert_eq!(&code, demo_ping::WASM_BINARY);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    log::info!("📗 Waiting for code approval");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Router(RouterEvent::CodeGotValidated {
                id: validated_code_id,
                valid,
            }) => {
                assert_eq!(code_id, validated_code_id);
                assert!(valid);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    let code = env
        .db
        .original_code(code_id)
        .expect("After approval, the code is guaranteed to be in the database");
    let _ = env
        .db
        .instrumented_code(1, code_id)
        .expect("After approval, instrumented code is guaranteed to be in the database");
    assert_eq!(code, demo_ping::WASM_BINARY);

    let _ = env
        .ethereum
        .router()
        .create_program(code_id, H256::random(), b"PING", 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for program create, PONG reply and program update");
    let mut program_id = ActorId::default();
    let mut init_message_id = MessageId::default();
    let mut reply_sent = false;
    let mut block_committed = None;
    listener
        .apply_until_block_event(|event| {
            match event {
                BlockEvent::Router(RouterEvent::ProgramCreated {
                    actor_id,
                    code_id: loaded_id,
                }) => {
                    assert_eq!(code_id, loaded_id);
                    program_id = actor_id;
                }
                BlockEvent::Mirror { address, event } => {
                    if address == program_id {
                        match event {
                            MirrorEvent::MessageQueueingRequested {
                                id,
                                source,
                                payload,
                                value,
                                ..
                            } => {
                                assert_eq!(source, env.sender_address);
                                assert_eq!(payload, b"PING");
                                assert_eq!(value, 0);
                                init_message_id = id;
                            }
                            MirrorEvent::Reply {
                                payload, reply_to, ..
                            } => {
                                assert_eq!(payload, b"PONG");
                                assert_eq!(reply_to, init_message_id);
                                reply_sent = true;
                            }
                            MirrorEvent::StateChanged { .. } => {
                                assert!(reply_sent);
                            }
                            _ => {}
                        }
                    }
                }
                BlockEvent::Router(RouterEvent::BlockCommitted { block_hash }) => {
                    block_committed = Some(block_hash);
                    return Ok(Some(()));
                }
                _ => {}
            }
            Ok(None)
        })
        .await
        .unwrap();

    let block_committed_on_router = env.router_query.last_commitment_block_hash().await.unwrap();
    assert_eq!(block_committed, Some(block_committed_on_router));

    // Mine some blocks to check deep sync.
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(150),
        }))
        .await
        .unwrap();

    // Send message in between.
    let program_address = ethexe_signer::Address::try_from(program_id).unwrap();

    let wvara = env.ethereum.router().wvara();

    log::info!("📗 Approving WVara to mirror");
    wvara.approve_all(program_address.0.into()).await.unwrap();

    let ping_program = env.ethereum.mirror(program_address);

    log::info!("📗 Sending PING message");
    let _tx = ping_program.send_message(b"PING", 0).await.unwrap();

    // Mine some blocks to check deep sync.
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(150),
        }))
        .await
        .unwrap();

    log::info!("📗 Waiting for PONG reply");
    listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror { address, event } => {
                if address == program_id {
                    if let MirrorEvent::Reply { payload, value, .. } = event {
                        assert_eq!(payload, b"PONG");
                        assert_eq!(value, 0);
                        return Ok(Some(()));
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        })
        .await
        .unwrap();
}
