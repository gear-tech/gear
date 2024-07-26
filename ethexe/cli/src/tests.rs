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
use ethexe_common::{db::CodesStorage, events::BlockEvent};
use ethexe_db::{Database, MemDb};
use ethexe_ethereum::{Ethereum, RouterQuery};
use ethexe_observer::{Event, MockBlobReader, Observer, Query};
use ethexe_processor::Processor;
use ethexe_sequencer::Sequencer;
use ethexe_signer::Signer;
use ethexe_validator::Validator;
use futures::StreamExt;
use gear_core::ids::prelude::*;
use gprimitives::{ActorId, CodeId, H256};
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
    block_time: Duration,
    running_service_handle: Option<JoinHandle<Result<()>>>,
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

        let db = Database::from_one(&MemDb::default());

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

        let router_address = ethereum.router().address();

        let router_query = RouterQuery::new(&rpc_url, router_address).await?;
        let genesis_block_hash = router_query.genesis_block_hash().await?;

        let blob_reader = blob_reader.unwrap_or_else(|| Arc::new(MockBlobReader::new(block_time)));

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

        let processor = Processor::new(self.db.clone())?;

        let sequencer = Sequencer::new(
            &ethexe_sequencer::Config {
                ethereum_rpc: self.rpc_url.clone(),
                sign_tx_public: self.sequencer_public_key,
                router_address: self.router_address,
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
            None,
            Some(sequencer),
            Some(validator),
            None,
            None,
        );

        let handle = task::spawn(service.run());
        self.running_service_handle = Some(handle);

        // Sleep to wait for the new service to start
        // TODO: find a better way to wait for the service to start
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
        let tx_hash = self.ethereum.router().upload_code(code_id, blob_tx).await?;

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

    let anvil = TestEnv::start_anvil();

    let mut env = TestEnv::new(TestEnvConfig::default().rpc_url(anvil.ws_endpoint()))
        .await
        .unwrap();
    let mut listener = env.new_listener().await;

    env.start_service().await.unwrap();

    let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

    log::info!("📗 Waiting for code loaded");
    listener
        .apply_until(|event| {
            if let Event::CodeLoaded(loaded) = event {
                assert_eq!(loaded.code_id, code_id);
                assert_eq!(loaded.code.as_slice(), demo_ping::WASM_BINARY);
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    log::info!("📗 Waiting for code approval");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::CodeApproved(approved) = event {
                assert_eq!(approved.code_id, code_id);
                Ok(Some(()))
            } else {
                Ok(None)
            }
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
        .create_program(code_id, H256::random(), b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for program create, PONG reply and program update");
    let mut reply_sent = false;
    let mut program_id = ActorId::default();
    let mut block_committed = None;
    listener
        .apply_until_block_event(|event| {
            match event {
                BlockEvent::CreateProgram(create) => {
                    assert_eq!(create.code_id, code_id);
                    assert_eq!(create.init_payload, b"PING");
                    assert_eq!(create.gas_limit, 10_000_000_000);
                    assert_eq!(create.value, 0);
                    program_id = create.actor_id;
                }
                BlockEvent::UserReplySent(reply) => {
                    assert_eq!(reply.value, 0);
                    assert_eq!(reply.payload, b"PONG");
                    reply_sent = true;
                }
                BlockEvent::UpdatedProgram(updated) => {
                    assert_eq!(updated.actor_id, program_id);
                    assert_eq!(updated.old_state_hash, H256::zero());
                    assert!(reply_sent);
                }
                BlockEvent::BlockCommitted(committed) => {
                    block_committed = Some(committed.block_hash);
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
    let ping_program = env.ethereum.program(program_address);
    let _tx = ping_program
        .send_message(b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for PONG reply");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::UserReplySent(reply) = event {
                assert_eq!(reply.value, 0);
                assert_eq!(reply.payload, b"PONG");
                Ok(Some(()))
            } else {
                Ok(None)
            }
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
        .apply_until(|event| {
            if let Event::CodeLoaded(loaded) = event {
                assert_eq!(loaded.code_id, code_id);
                assert_eq!(loaded.code.as_slice(), demo_ping::WASM_BINARY);
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    log::info!("📗 Waiting for code approval");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::CodeApproved(approved) = event {
                assert_eq!(approved.code_id, code_id);
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    log::info!("📗 Abort service to simulate node blocks skipping");
    env.stop_service().await.unwrap();

    let _ = env
        .ethereum
        .router()
        .create_program(code_id, H256::random(), b"PING", 10_000_000_000, 0)
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
    let mut reply_sent = false;
    let mut program_id = ActorId::default();
    listener
        .apply_until_block_event(|event| {
            match event {
                BlockEvent::CreateProgram(create) => {
                    assert_eq!(create.code_id, code_id);
                    assert_eq!(create.init_payload, b"PING");
                    assert_eq!(create.gas_limit, 10_000_000_000);
                    assert_eq!(create.value, 0);
                    program_id = create.actor_id;
                }
                BlockEvent::UserReplySent(reply) => {
                    assert_eq!(reply.value, 0);
                    assert_eq!(reply.payload, b"PONG");
                    reply_sent = true;
                }
                BlockEvent::UpdatedProgram(updated) => {
                    assert_eq!(updated.actor_id, program_id);
                    assert_eq!(updated.old_state_hash, H256::zero());
                    assert!(reply_sent);
                }
                BlockEvent::BlockCommitted(_) => {
                    return Ok(Some(()));
                }
                _ => {}
            }
            Ok(None)
        })
        .await
        .unwrap();

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        provider.get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = provider.anvil_snapshot().await.unwrap();

    let program_address = ethexe_signer::Address::try_from(program_id).unwrap();
    let ping_program = env.ethereum.program(program_address);

    log::info!("📗 Sending PING message");
    let _tx = ping_program
        .send_message(b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for PONG reply");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::UserReplySent(reply) = event {
                assert_eq!(reply.value, 0);
                assert_eq!(reply.payload, b"PONG");
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    // Await for service block with user reply handling
    // TODO: this is for better logs reading only, should find a better solution.
    tokio::time::sleep(env.block_time).await;

    log::info!("📗 Reverting to the program creation snapshot");
    provider
        .anvil_revert(program_created_snapshot_id)
        .await
        .map(|res| assert!(res))
        .unwrap();

    log::info!("📗 Sending PING message after reorg");
    let _tx = ping_program
        .send_message(b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for PONG reply after reorg");
    listener
        .apply_until_block_event(|event| {
            if let BlockEvent::UserReplySent(reply) = event {
                assert_eq!(reply.value, 0);
                assert_eq!(reply.payload, b"PONG");
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    // The last step is to test correctness after db cleanup
    let router_address = env.router_address;
    let blob_reader = env.blob_reader.clone();
    let validator_private_key = env.validator_private_key;
    drop(env);

    log::info!("📗 Sending PING message, db cleanup and service shuting down");
    let _tx = ping_program
        .send_message(b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

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
        .apply_until_block_event(|event| {
            if let BlockEvent::UserReplySent(reply) = event {
                assert_eq!(reply.reply_details.to_message_id(), Default::default());
                assert_eq!(reply.value, 0);
                assert_eq!(reply.payload, b"PONG");
                Ok(Some(()))
            } else {
                Ok(None)
            }
        })
        .await
        .unwrap();

    // Await for service block with user reply handling
    // TODO: this is for better logs reading only, should find a better solution.
    tokio::time::sleep(Duration::from_secs(1)).await;

    log::info!("📗 Done");
}
