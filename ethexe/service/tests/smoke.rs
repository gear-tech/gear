// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use ethexe_common::consensus::{DEFAULT_BATCH_SIZE_LIMIT, DEFAULT_CHAIN_DEEPNESS_THRESHOLD};
use ethexe_ethereum::Ethereum;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RpcConfig};
use ethexe_service::{
    Service,
    config::{self, Config, EthereumConfig},
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
        dev: false,
        pre_funded_accounts: 10,
        fast_sync: false,
        chain_deepness_threshold: DEFAULT_CHAIN_DEEPNESS_THRESHOLD,
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
        blob_gas_multiplier: Ethereum::NO_BLOB_GAS_MULTIPLIER,
    };

    let mut config = Config {
        node: node_cfg,
        ethereum: eth_cfg,
        network: None,
        rpc: None,
        prometheus: None,
    };

    let service = Service::new(&config).await.unwrap();
    drop(service);

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
