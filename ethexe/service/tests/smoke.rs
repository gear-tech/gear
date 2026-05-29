// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::consensus::DEFAULT_BATCH_SIZE_LIMIT;
use ethexe_ethereum::{Ethereum, router::RouterQuery};
use ethexe_malachite::malachite_libp2p_peer_id;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RpcConfig};
use ethexe_service::{
    Service,
    config::{self, Config, EthereumConfig, ValidatorIdentity},
};
use gsigner::secp256k1::Signer;
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tempfile::tempdir;

#[tokio::test]
async fn constructor() {
    let tmp_dir = tempdir().unwrap();
    let tmp_dir = tmp_dir.path().to_path_buf();
    let key_path = tmp_dir.join("key");
    let network_key_path = tmp_dir.join("net");

    let network_key = Signer::fs(network_key_path)
        .expect("failed to create signer")
        .generate()
        .unwrap();

    let node_cfg = config::NodeConfig {
        database_path: tmp_dir.join("db"),
        key_path,
        validator: Default::default(),
        validator_session: Default::default(),
        eth_max_sync_depth: 1_000,
        worker_threads: None,
        blocking_threads: None,
        chunk_processing_threads: 16,
        block_gas_limit: 4_000_000_000_000,
        canonical_quarantine: 0,
        post_quarantine_delay: 0,
        dev: false,
        pre_funded_accounts: 10,
        fast_sync: false,
        coordinator_aggregation_delay: Duration::from_millis(1500),
        uncommitted_chain_len_threshold: std::num::NonZero::new(500).unwrap(),
        commitment_delay_limit: ethexe_common::DEFAULT_COMMITMENT_DELAY_LIMIT,
        batch_size_limit: DEFAULT_BATCH_SIZE_LIMIT,
        genesis_state_dump: None,
    };

    let eth_cfg = EthereumConfig {
        rpc: "wss://hoodi-reth-rpc.gear-tech.io/ws".into(),
        beacon_rpc: "https://hoodi-lighthouse-rpc.gear-tech.io".into(),
        router_address: "0xE549b0AfEdA978271FF7E712232B9F7f39A0b060"
            .parse()
            .expect("infallible"),
        block_time: Duration::from_secs(12),
        eip1559_fee_increase_percentage: Ethereum::NO_EIP1559_FEE_INCREASE_PERCENTAGE,
        eip1559_max_fee_per_gas_in_gwei: Ethereum::NO_EIP1559_MAX_FEE_PER_GAS_IN_GWEI,
        blob_gas_multiplier: Ethereum::NO_BLOB_GAS_MULTIPLIER,
    };

    // `Service::new` resolves the Malachite validator set by looking
    // each on-chain validator address up in
    // `config.malachite.validator_identities`. The smoke test only
    // exercises the constructor wiring (the service is dropped
    // immediately, nothing signs anything), so populate the table with
    // freshly generated identities keyed by the live router's validators.
    let malachite_signer =
        Signer::fs(tmp_dir.join("malachite-identities")).expect("failed to create signer");
    let router_query = RouterQuery::new(&eth_cfg.rpc, eth_cfg.router_address)
        .await
        .expect("router query");
    let validators = router_query.validators().await.expect("validators");
    let validator_identities = validators
        .iter()
        .map(|addr| {
            let public_key = malachite_signer
                .generate()
                .expect("failed to generate malachite pub key");
            let secret = malachite_signer
                .private_key(public_key)
                .expect("failed to load malachite private key");
            (
                *addr,
                ValidatorIdentity {
                    public_key,
                    peer_id: malachite_libp2p_peer_id(&secret.to_bytes()),
                },
            )
        })
        .collect();

    let mut config = Config {
        node: node_cfg,
        ethereum: eth_cfg,
        network: None,
        malachite: config::MalachiteCliConfig {
            validator_identities,
            ..Default::default()
        },
        rpc: None,
        prometheus: None,
    };

    let service = Service::new(&config).await.unwrap();
    drop(service);

    // Service no longer releases its RocksDB / libp2p / Malachite WAL
    // synchronously on drop (the Malachite engine keeps a background
    // app task; only `MalachiteService::shutdown().await` joins it).
    // The constructor smoke test doesn't run the service, so move the
    // second build onto a fresh database path instead of waiting for
    // the first to fully unwind.
    config.node.database_path = tmp_dir.join("db2");

    // Enable all optional services
    config.network = Some(ethexe_network::NetworkConfig::new_local(
        network_key,
        config.ethereum.router_address,
    ));

    config.rpc = Some(RpcConfig {
        listen_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9944),
        cors: None,
        gas_allowance: DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER
            .checked_mul(config.node.block_gas_limit)
            .unwrap(),
        chunk_size: config.node.chunk_processing_threads,
        with_dev_api: false,
    });

    config.prometheus = Some(PrometheusConfig {
        name: "DevNode".into(),
        addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9635),
    });

    Service::new(&config).await.unwrap();
}
