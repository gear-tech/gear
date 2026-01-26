// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::tests::utils::{
    EnvNetworkConfig, NodeConfig, TestEnv, TestEnvConfig, ValidatorsConfig, init_logger,
};
use ethexe_common::{ecdsa::ContractSignature, gear::BatchCommitment};
use ethexe_consensus::BatchCommitter;
use ethexe_ethereum::{TryGetReceipt, router::Router};
use gprimitives::H256;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn commit_rejected_with_bad_frost_signature() {
    init_logger();

    #[derive(Clone)]
    struct BadSignatureCommitter {
        router: Router,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for BadSignatureCommitter {
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
            batch: BatchCommitment,
            mut signature96: [u8; 96],
        ) -> anyhow::Result<H256> {
            signature96[0] ^= 0x01;
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

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(1),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validator =
        env.new_node(NodeConfig::named("validator").validator(env.validators[0].clone()));
    validator.start_service().await;

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

    validator.stop_service().await;
    validator.custom_committer = Some(Box::new(BadSignatureCommitter {
        router: env.ethereum.router(),
    }));
    validator.start_service().await;

    let pending = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap();

    timeout(env.block_time * 5, pending.wait_for())
        .await
        .expect_err("Timeout expected due to bad signature");
}
