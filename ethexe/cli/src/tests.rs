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
use alloy::node_bindings::Anvil;
use anyhow::{anyhow, Result};
use ethexe_common::events::BlockEvent;
use ethexe_db::{Database, MemDb};
use ethexe_ethereum::{Ethereum, RouterQuery};
use ethexe_observer::{Event, MockBlobReader, Observer, Query};
use ethexe_processor::Processor;
use ethexe_sequencer::Sequencer;
use ethexe_signer::Signer;
use ethexe_validator::Validator;
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use std::sync::Arc;

struct Listener {
    receiver: tokio::sync::mpsc::Receiver<Event>,
    _handle: tokio::task::JoinHandle<()>,
}

impl Listener {
    pub fn new(mut observer: Observer) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel::<Event>(8 * 1024 * 1024);

        let _handle = tokio::task::spawn(async move {
            let observer_events = observer.events();
            futures::pin_mut!(observer_events);

            while let Some(event) = observer_events.next().await {
                sender.send(event).await.unwrap();
            }
        });

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

struct TestEnv {
    pub _db: Database,
    pub blob_reader: Arc<MockBlobReader>,
    pub observer: Observer,
    pub ethereum: Ethereum,
    pub _router_query: RouterQuery,
    service: Option<Service>,
}

impl TestEnv {
    async fn new(rpc: String) -> Result<TestEnv> {
        let db = Database::from_one(&MemDb::default());

        let net_config = ethexe_network::NetworkConfiguration::new_local();
        let network = ethexe_network::NetworkWorker::new(net_config)?;

        let tempdir = tempfile::tempdir()?;
        let signer = Signer::new(tempdir.into_path())?;
        let sender_public_key = signer.add_key(
            // Anvil account (0) with balance
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?,
        )?;
        let validator_public_key = signer.generate_key()?;
        let sequencer_public_key = signer.add_key(
            // Anvil account (1) with balance
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".parse()?,
        )?;

        let sender_address = sender_public_key.to_address();
        let validators = vec![validator_public_key.to_address()];
        let ethereum = Ethereum::deploy(&rpc, validators, signer.clone(), sender_address).await?;
        let blob_reader = Arc::new(MockBlobReader::default());

        let router_address = ethereum.router().address();

        let router_query = RouterQuery::new(&rpc, router_address).await?;
        let genesis_block_hash = router_query.genesis_block_hash().await?;

        let query = Query::new(
            Box::new(db.clone()),
            &rpc,
            router_address,
            genesis_block_hash,
            blob_reader.clone(),
            10000,
        )
        .await?;

        let processor = Processor::new(db.clone())?;

        let sequencer = Sequencer::new(
            &ethexe_sequencer::Config {
                ethereum_rpc: rpc.clone(),
                sign_tx_public: sequencer_public_key,
                router_address,
            },
            signer.clone(),
        )
        .await?;

        let validator = Validator::new(
            &ethexe_validator::Config {
                pub_key: validator_public_key,
                router_address,
            },
            signer.clone(),
        );

        let observer = Observer::new(&rpc, router_address, blob_reader.clone())
            .await
            .expect("failed to create observer");

        let rpc = ethexe_rpc::RpcService::new(9090, db.clone());

        let service = Service::new_from_parts(
            db.clone(),
            network,
            observer.clone(),
            query,
            processor,
            signer,
            Some(sequencer),
            Some(validator),
            None,
            rpc,
        );

        let env = TestEnv {
            _db: db,
            blob_reader,
            observer,
            ethereum,
            _router_query: router_query,
            service: Some(service),
        };

        Ok(env)
    }

    pub fn new_listener(&self) -> Listener {
        Listener::new(self.observer.clone())
    }

    pub async fn upload_code(&self, code: &[u8]) -> Result<(H256, CodeId)> {
        let (tx_hash, code_id) = self
            .ethereum
            .router()
            .upload_code_with_sidecar(code)
            .await?;
        self.blob_reader
            .add_blob_transaction(tx_hash, code.to_vec())
            .await;
        Ok((tx_hash, code_id))
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping() {
    let _ = env_logger::try_init();

    let anvil = Anvil::new().try_spawn().unwrap();

    let mut env = TestEnv::new(anvil.ws_endpoint()).await.unwrap();
    let mut listener = env.new_listener();

    let service = env.service.take().unwrap();
    let _ = tokio::task::spawn(service.run());

    let (_, code_id) = env.upload_code(demo_ping::WASM_BINARY).await.unwrap();

    log::info!("📗 Waiting for code loaded");
    listener
        .apply_until(|event| {
            if let Event::CodeLoaded(loaded) = event {
                assert_eq!(loaded.code_id, code_id);
                assert_eq!(loaded.code.as_slice(), demo_ping::WASM_BINARY);
                Ok(Some(()))
            } else {
                return Ok(None);
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

    let _ = env
        .ethereum
        .router()
        .create_program(code_id, H256::random(), b"PING", 10_000_000_000, 0)
        .await
        .unwrap();

    log::info!("📗 Waiting for program create, PONG reply and program update");
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
                    return Ok(Some(()));
                }
                _ => {}
            }
            Ok(None)
        })
        .await
        .unwrap();

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
