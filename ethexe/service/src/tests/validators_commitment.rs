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

//! Validators commitment integration tests.

use crate::tests::utils::{
    EnvNetworkConfig, InfiniteStreamExt, NodeConfig, ObserverEventReceiver, TestEnv, TestEnvConfig,
    ValidatorsConfig, Wallets, init_logger,
};
use alloy::providers::{Provider as _, ext::AnvilApi};
use ethexe_common::{
    ContractSignature,
    crypto::{DkgPublicKeyPackage, DkgSessionId, DkgVssCommitment},
    db::{DkgSessionState, DkgStorageRW},
    ecdsa::PublicKey,
    events::{BlockEvent, RouterEvent},
    gear::{AggregatedPublicKey, BatchCommitment},
};
use ethexe_consensus::BatchCommitter;
use ethexe_ethereum::{TryGetReceipt, deploy::ContractsDeploymentParams, router::Router};
use gprimitives::{H256, U256};
use gsigner::secp256k1::Signer;
use std::sync::Arc;
use tokio::sync::Mutex;

async fn run_validators_commitment_with_committer_and_events(
    committer_factory: impl Fn(Router) -> Box<dyn BatchCommitter>,
) -> (TestEnv, ObserverEventReceiver) {
    let election_ts = 20 * 60 * 60;
    let era_duration = 24 * 60 * 60;
    let deploy_params = ContractsDeploymentParams {
        with_middleware: true,
        era_duration,
        election_duration: era_duration - election_ts,
    };

    let signer = Signer::memory();
    let mut wallets = Wallets::anvil(&signer);

    let current_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();
    let next_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();

    let env_config = TestEnvConfig {
        validators: ValidatorsConfig::ProvidedValidators(current_validators),
        deploy_params,
        network: EnvNetworkConfig::Enabled,
        signer: signer.clone(),
        ..Default::default()
    };
    let mut env = TestEnv::new(env_config).await.unwrap();

    let genesis_block_hash = env
        .ethereum
        .router()
        .query()
        .genesis_block_hash()
        .await
        .unwrap();
    let genesis_ts = env
        .provider
        .get_block_by_hash(genesis_block_hash.0.into())
        .await
        .unwrap()
        .unwrap()
        .header
        .timestamp;

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.custom_committer = Some(committer_factory(env.ethereum.router()));
        validator.start_service().await;
        validators.push(validator);
    }

    let (next_validators_configs, _commitment) =
        TestEnv::define_session_keys_for_era(&signer, next_validators, 1);
    let next_public_key_package = next_validators_configs[0].dkg_public_key_package.clone();
    let next_vss_commitment = next_validators_configs[0].dkg_vss_commitment.clone();
    let next_dkg_session = DkgSessionId { era: 1 };

    let next_validators: Vec<_> = next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.election_provider
        .set_predefined_election_at(
            election_ts + genesis_ts,
            next_validators.try_into().unwrap(),
        )
        .await;

    env.provider
        .anvil_set_next_block_timestamp(election_ts + genesis_ts)
        .await
        .unwrap();

    for validator in validators.iter_mut() {
        validator
            .db
            .set_public_key_package(next_dkg_session.era, next_public_key_package.clone());
        validator
            .db
            .set_dkg_vss_commitment(next_dkg_session.era, next_vss_commitment.clone());
        validator.db.set_dkg_session_state(
            next_dkg_session,
            DkgSessionState {
                completed: true,
                ..Default::default()
            },
        );
    }

    let observer_events = env.new_observer_events();
    env.force_new_block().await;
    (env, observer_events)
}

async fn run_validators_commitment_with_committer(
    committer_factory: impl Fn(Router) -> Box<dyn BatchCommitter>,
) -> TestEnv {
    let (env, _) = run_validators_commitment_with_committer_and_events(committer_factory).await;
    env
}

fn aggregated_key_from_package(package: &DkgPublicKeyPackage) -> AggregatedPublicKey {
    let public_key_compressed: [u8; 33] = package
        .verifying_key()
        .serialize()
        .expect("verifying key serialization failed")
        .try_into()
        .expect("invalid aggregated public key length");
    let public_key_uncompressed = PublicKey::from_bytes(public_key_compressed)
        .expect("valid aggregated public key")
        .to_uncompressed();
    let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

    AggregatedPublicKey {
        x: U256::from_big_endian(public_key_x_bytes),
        y: U256::from_big_endian(public_key_y_bytes),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_wrong_validators() {
    init_logger();

    #[derive(Clone)]
    struct WrongValidatorsCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for WrongValidatorsCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            if let Some(commitment) = batch.validators_commitment.as_mut() {
                let mut validators: Vec<_> = commitment.validators.iter().copied().collect();
                if validators.len() > 1 {
                    let last = validators.len() - 1;
                    validators.swap(0, last);
                    commitment.validators = validators
                        .try_into()
                        .expect("validators list must remain non-empty");
                }
            }
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env = run_validators_commitment_with_committer(|router| {
        Box::new(WrongValidatorsCommitter { router })
    })
    .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with wrong validators");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_bad_vss() {
    init_logger();

    #[derive(Clone)]
    struct BadVssCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for BadVssCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            if let Some(commitment) = batch.validators_commitment.as_mut() {
                let mut serialized = commitment
                    .verifiable_secret_sharing_commitment
                    .serialize()
                    .unwrap();
                if let Some(first_coeff) = serialized.first_mut()
                    && let Some(first_byte) = first_coeff.first_mut()
                {
                    *first_byte ^= 0x01;
                }
                commitment.verifiable_secret_sharing_commitment =
                    DkgVssCommitment::deserialize(serialized).unwrap();
            }
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env =
        run_validators_commitment_with_committer(|router| Box::new(BadVssCommitter { router }))
            .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with bad VSS commitment");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_wrong_era() {
    init_logger();

    #[derive(Clone)]
    struct WrongEraCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for WrongEraCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            if let Some(commitment) = batch.validators_commitment.as_mut() {
                commitment.era_index = commitment.era_index.saturating_add(1);
            }
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env =
        run_validators_commitment_with_committer(|router| Box::new(WrongEraCommitter { router }))
            .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with wrong era index");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_bad_public_key() {
    init_logger();

    #[derive(Clone)]
    struct BadPublicKeyCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for BadPublicKeyCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            if let Some(commitment) = batch.validators_commitment.as_mut() {
                commitment.aggregated_public_key = AggregatedPublicKey {
                    x: U256::from(1u64),
                    y: commitment.aggregated_public_key.y,
                };
            }
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env = run_validators_commitment_with_committer(|router| {
        Box::new(BadPublicKeyCommitter { router })
    })
    .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with bad aggregated public key");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_pk_mismatch() {
    init_logger();

    let alt_signer = Signer::memory();
    let mut wallets = Wallets::anvil(&alt_signer);
    let alt_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();
    let (alt_configs, _) = TestEnv::define_session_keys_for_era(&alt_signer, alt_validators, 1);
    let mismatched_key = aggregated_key_from_package(&alt_configs[0].dkg_public_key_package);

    #[derive(Clone)]
    struct PkMismatchCommitter {
        router: Router,
        mismatched_key: AggregatedPublicKey,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for PkMismatchCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            if let Some(commitment) = batch.validators_commitment.as_mut() {
                commitment.aggregated_public_key = self.mismatched_key;
            }
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env = run_validators_commitment_with_committer(|router| {
        Box::new(PkMismatchCommitter {
            router,
            mismatched_key: mismatched_key.clone(),
        })
    })
    .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with mismatched public key package");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn validators_commitment_rejected_with_wrong_block_hash() {
    init_logger();

    #[derive(Clone)]
    struct WrongBlockHashCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for WrongBlockHashCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }

        async fn commit_frost(
            self: Box<Self>,
            mut batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            batch.block_hash = H256::random();
            let pending = self
                .router
                .commit_batch_frost_pending(batch, signature96)
                .await?;
            pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let env = run_validators_commitment_with_committer(|router| {
        Box::new(WrongBlockHashCommitter { router })
    })
    .await;

    tokio::time::timeout(
        env.block_time * 5,
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .expect_err("Commitment should be rejected with wrong block hash");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn duplicate_validators_commitment_is_rejected() {
    init_logger();

    #[derive(Clone)]
    enum CommitRecord {
        Frost(BatchCommitment, [u8; 96]),
        Ecdsa(BatchCommitment, Vec<ContractSignature>),
    }

    #[derive(Clone)]
    struct DuplicateCommitter {
        router: Router,
        record: Arc<Mutex<Option<CommitRecord>>>,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for DuplicateCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            let batch_clone = batch.clone();
            let signatures_clone = signatures.clone();

            let mut record = self.record.lock().await;
            if record.is_none() {
                *record = Some(CommitRecord::Ecdsa(batch_clone, signatures_clone));
            }
            drop(record);

            let pending = self.router.commit_batch_pending(batch, signatures).await?;
            let receipt = pending
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())?;

            Ok(receipt)
        }

        async fn commit_frost(
            self: Box<Self>,
            batch: BatchCommitment,
            signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            let mut record = self.record.lock().await;
            if record.is_none() {
                *record = Some(CommitRecord::Frost(batch.clone(), signature96));
            }
            drop(record);

            let pending = self
                .router
                .commit_batch_frost_pending(batch.clone(), signature96)
                .await?;
            let receipt = pending.try_get_receipt_check_reverted().await?;

            Ok(receipt.transaction_hash.0.into())
        }
    }

    let record = Arc::new(Mutex::new(None));
    let record_handle = record.clone();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(1),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let genesis_block_hash = env
        .ethereum
        .router()
        .query()
        .genesis_block_hash()
        .await
        .unwrap();
    let genesis_ts = env
        .provider
        .get_block_by_hash(genesis_block_hash.0.into())
        .await
        .unwrap()
        .unwrap()
        .header
        .timestamp;

    let mut validator =
        env.new_node(NodeConfig::named("validator").validator(env.validators[0].clone()));
    validator.custom_committer = Some(Box::new(DuplicateCommitter {
        router: env.ethereum.router(),
        record: record.clone(),
    }));
    validator.start_service().await;

    let next_validators: Vec<_> = vec![env.validators[0].public_key];
    let (next_configs, _commitment) =
        TestEnv::define_session_keys_for_era(&env.signer, next_validators, 1);
    let next_public_key_package = next_configs[0].dkg_public_key_package.clone();
    let next_vss_commitment = next_configs[0].dkg_vss_commitment.clone();
    let next_dkg_session = DkgSessionId { era: 1 };

    let next_addresses: Vec<_> = next_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.election_provider
        .set_predefined_election_at(
            20 * 60 * 60 + genesis_ts,
            next_addresses.try_into().unwrap(),
        )
        .await;

    env.provider
        .anvil_set_next_block_timestamp(20 * 60 * 60 + genesis_ts)
        .await
        .unwrap();

    validator
        .db
        .set_public_key_package(next_dkg_session.era, next_public_key_package);
    validator
        .db
        .set_dkg_vss_commitment(next_dkg_session.era, next_vss_commitment);
    validator.db.set_dkg_session_state(
        next_dkg_session,
        DkgSessionState {
            completed: true,
            ..Default::default()
        },
    );

    env.force_new_block().await;

    let initial_hash = env
        .ethereum
        .router()
        .query()
        .latest_committed_batch_hash()
        .await
        .unwrap();

    let uploaded_code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    let ping_actor = env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let pending = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap();

    tokio::time::timeout(env.block_time * 10, pending.wait_for())
        .await
        .expect("expected ping reply")
        .unwrap();

    let record = tokio::time::timeout(env.block_time * 30, async {
        loop {
            if let Some(record) = record_handle.lock().await.clone() {
                break record;
            }
            env.force_new_block().await;
            tokio::time::sleep(env.block_time / 2).await;
        }
    })
    .await
    .expect("commit batch should be recorded");

    tokio::time::timeout(env.block_time * 10, async {
        loop {
            let hash = env
                .ethereum
                .router()
                .query()
                .latest_committed_batch_hash()
                .await
                .unwrap();
            if hash != initial_hash {
                break;
            }
            tokio::time::sleep(env.block_time / 2).await;
        }
    })
    .await
    .expect("initial validators commitment should succeed");

    match record {
        CommitRecord::Frost(batch, signature96) => {
            let duplicate = env
                .ethereum
                .router()
                .commit_batch_frost_pending(batch, signature96)
                .await;
            match duplicate {
                Ok(pending) => {
                    let err = pending
                        .try_get_receipt_check_reverted()
                        .await
                        .expect_err("duplicate commitment unexpectedly succeeded");
                    assert!(
                        err.to_string()
                            .contains("invalid previous committed batch hash"),
                        "unexpected revert reason: {err}"
                    );
                }
                Err(err) => {
                    assert!(
                        err.to_string()
                            .contains("invalid previous committed batch hash"),
                        "unexpected submit error: {err}"
                    );
                }
            }
        }
        CommitRecord::Ecdsa(batch, signatures) => {
            let duplicate = env
                .ethereum
                .router()
                .commit_batch_pending(batch, signatures)
                .await;
            match duplicate {
                Ok(pending) => {
                    let err = pending
                        .try_get_receipt_check_reverted()
                        .await
                        .expect_err("duplicate commitment unexpectedly succeeded");
                    assert!(
                        err.to_string()
                            .contains("invalid previous committed batch hash"),
                        "unexpected revert reason: {err}"
                    );
                }
                Err(err) => {
                    assert!(
                        err.to_string()
                            .contains("invalid previous committed batch hash"),
                        "unexpected submit error: {err}"
                    );
                }
            }
        }
    }
}
