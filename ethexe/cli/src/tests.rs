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
use utils::*;

struct PingTest {
    listener: Listener,
    code_id: Option<CodeId>,
    program_id: Option<ActorId>,
}

impl PingTest {
    async fn new(env: &mut TestEnv) -> Self {
        let listener = env.new_listener().await;
        Self {
            listener,
            code_id: None,
            program_id: None,
        }
    }

    async fn upload_code(&mut self, env: &mut TestEnv) {
        assert!(self.code_id.is_none(), "Code already uploaded");

        log::info!("🏓 Upload code and waiting for code loaded and validated");

        let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

        self.listener
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

        self.listener
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

        self.code_id = Some(code_id);
    }

    async fn create_program(&mut self, env: &mut TestEnv) {
        assert!(self.program_id.is_none(), "Program already created");
        let code_id = self.code_id.expect("Code must be uploaded first");

        log::info!("🏓 Create ping program");

        let _ = env
            .ethereum
            .router()
            .create_program(code_id, H256::random(), b"PING", 0)
            .await
            .unwrap();
    }

    async fn wait_for_program_creation(&mut self, env: &mut TestEnv) {
        assert!(self.program_id.is_none(), "Program already created");
        let code_id = self.code_id.expect("Code must be uploaded first");

        log::info!("🏓 Waiting for program creation and PONG reply");

        let mut program_id = ActorId::default();
        let mut init_message_id = MessageId::default();
        let mut reply_sent = false;
        let mut block_committed = None;
        self.listener
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

        let block_committed_on_router =
            env.router_query.last_commitment_block_hash().await.unwrap();
        assert_eq!(block_committed, Some(block_committed_on_router));

        self.program_id = Some(program_id);
    }

    async fn approve_wvara(&mut self, env: &mut TestEnv) {
        let program_id = self.program_id.expect("Program must be created first");

        log::info!("🏓 Approving WVara to mirror");

        let program_address = ethexe_signer::Address::try_from(program_id).unwrap();
        let wvara = env.ethereum.router().wvara();
        wvara.approve_all(program_address.0.into()).await.unwrap();
    }

    async fn send_ping(&mut self, env: &mut TestEnv) {
        let program_id = self.program_id.expect("Program must be created first");

        log::info!("🏓 Sending PING message");

        let program_address = ethexe_signer::Address::try_from(program_id).unwrap();
        let ping_program = env.ethereum.mirror(program_address);

        let _tx = ping_program.send_message(b"PING", 0).await.unwrap();
    }

    async fn wait_for_pong_reply(&mut self) {
        let program_id = self.program_id.expect("Program must be created first");

        log::info!("🏓 Waiting for PONG reply");

        self.listener
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
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1).await.unwrap();
    let mut ping = PingTest::new(&mut env).await;
    let mut node = env.create_node(None);
    node.start_service(Some(env.wallets.next()), Some(env.validators[0]), None)
        .await;

    ping.upload_code(&mut env).await;

    let code_id = ping.code_id.expect("Code id must be set");
    let code = node
        .db
        .original_code(code_id)
        .expect("After approval, the code is guaranteed to be in the database");
    let _ = node
        .db
        .instrumented_code(1, code_id)
        .expect("After approval, instrumented code is guaranteed to be in the database");
    assert_eq!(code, demo_ping::WASM_BINARY);

    ping.create_program(&mut env).await;
    ping.wait_for_program_creation(&mut env).await;
    ping.approve_wvara(&mut env).await;

    ping.send_ping(&mut env).await;
    ping.wait_for_pong_reply().await;

    ping.send_ping(&mut env).await;
    ping.wait_for_pong_reply().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_reorg() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1).await.unwrap();
    let mut ping = PingTest::new(&mut env).await;
    let mut node = env.create_node(None);
    let sequencer_pub_key = env.wallets.next();
    node.start_service(Some(sequencer_pub_key), Some(env.validators[0]), None)
        .await;

    let provider = env.observer.provider().clone();

    ping.upload_code(&mut env).await;

    log::info!("📗 Abort service to simulate node blocks skipping");
    node.stop_service().await;

    ping.create_program(&mut env).await;

    // Mine some blocks to check missed blocks support
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(10),
        }))
        .await
        .unwrap();

    // Start new service
    node.start_service(Some(sequencer_pub_key), Some(env.validators[0]), None)
        .await;

    // IMPORTANT: Mine one block to sent block event to the new service.
    provider.evm_mine(None).await.unwrap();

    ping.wait_for_program_creation(&mut env).await;
    ping.approve_wvara(&mut env).await;

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        provider.get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = provider.anvil_snapshot().await.unwrap();

    ping.send_ping(&mut env).await;
    ping.wait_for_pong_reply().await;

    // Await for service block with user reply handling
    // TODO: this is for better logs reading only, should find a better solution #4099
    tokio::time::sleep(env.block_time).await;

    log::info!("📗 Test after reverting to the program creation snapshot");
    provider
        .anvil_revert(program_created_snapshot_id)
        .await
        .map(|res| assert!(res))
        .unwrap();

    ping.send_ping(&mut env).await;
    ping.wait_for_pong_reply().await;

    // The last step is to test correctness after db cleanup
    node.stop_service().await;
    node.db = Database::from_one(&MemDb::default(), env.router_address.0);

    log::info!("📗 Test after db cleanup and service shutting down");
    ping.send_ping(&mut env).await;

    // Skip some blocks to simulate long time without service
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(10),
        }))
        .await
        .unwrap();

    node.start_service(Some(sequencer_pub_key), Some(env.validators[0]), None)
        .await;

    // Important: mine one block to sent block event to the new service.
    provider.evm_mine(None).await.unwrap();

    ping.wait_for_pong_reply().await;

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

    let mut env = TestEnv::new(1).await.unwrap();
    let mut ping = PingTest::new(&mut env).await;
    let mut node = env.create_node(None);
    let sequencer_pub_key = env.wallets.next();
    node.start_service(Some(sequencer_pub_key), Some(env.validators[0]), None)
        .await;

    let provider = env.observer.provider().clone();

    ping.upload_code(&mut env).await;

    ping.create_program(&mut env).await;
    ping.wait_for_program_creation(&mut env).await;

    // Mine some blocks to check deep sync.
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(150),
        }))
        .await
        .unwrap();

    ping.approve_wvara(&mut env).await;

    ping.send_ping(&mut env).await;

    // Mine some blocks to check deep sync.
    provider
        .evm_mine(Some(MineOptions::Options {
            timestamp: None,
            blocks: Some(150),
        }))
        .await
        .unwrap();

    ping.wait_for_pong_reply().await;
}

mod utils {
    use super::*;

    pub struct Listener {
        receiver: Receiver<Event>,
        _handle: JoinHandle<()>,
    }

    impl Listener {
        pub async fn new(mut observer: Observer) -> Self {
            let (sender, receiver) = mpsc::channel::<Event>(1024);

            let (send_subscription_created, receive_subscription_created) =
                oneshot::channel::<()>();
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

    pub struct AnvilWallets {
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
            *pub_key
        }
    }

    pub struct TestEnv {
        pub rpc_url: String,
        pub wallets: AnvilWallets,
        pub observer: Observer,
        pub blob_reader: Arc<MockBlobReader>,
        pub ethereum: Ethereum,
        pub router_query: RouterQuery,
        pub signer: Signer,
        pub validators: Vec<ethexe_signer::PublicKey>,
        pub router_address: ethexe_signer::Address,
        pub sender_id: ActorId,
        pub genesis_block_hash: H256,
        pub threshold: u64,
        pub block_time: Duration,

        _anvil: Option<AnvilInstance>,
    }

    impl TestEnv {
        pub async fn new(validators_amount: usize) -> Result<Self> {
            let (rpc_url, anvil) = match std::env::var("__ETHEXE_CLI_TESTS_RPC_URL") {
                Ok(rpc_url) => {
                    log::info!("📍 Using provided RPC URL: {}", rpc_url);
                    (rpc_url, None)
                }
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

            Ok(TestEnv {
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

        pub fn create_node(&self, db: Option<Database>) -> Node {
            Node {
                db: db.unwrap_or_else(|| {
                    Database::from_one(&MemDb::default(), self.router_address.0)
                }),
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

    pub struct Node {
        pub db: Database,

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
}
