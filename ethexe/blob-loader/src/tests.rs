// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use alloy::node_bindings::Anvil;
use ethexe_common::{
    CodeBlobInfo,
    db::{CodesStorageRW, OnChainStorageRW},
    gear_core::ids::prelude::CodeIdExt,
};
use ethexe_db::Database;
use ethexe_ethereum::deploy::EthereumDeployer;
use futures::StreamExt;
use gsigner::secp256k1::{PrivateKey, Signer};
use std::{collections::VecDeque, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
    time::{Duration, timeout},
};

const ATTEMPTS: NonZero<u8> = NonZero::new(3).unwrap();
const BLOB_CHUNK_SIZE: usize = 128 * 1024;

fn generated_code(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

fn set_blob_info(db: &Database, code_id: CodeId, tx_hash: H256) {
    db.set_code_blob_info(
        code_id,
        CodeBlobInfo {
            timestamp: 0,
            tx_hash,
        },
    );
}

async fn test_reader(
    ethereum_rpc: String,
    ethereum_beacon_rpc: String,
) -> ConsensusLayerBlobReader {
    test_reader_with_block_time(ethereum_rpc, ethereum_beacon_rpc, Duration::from_millis(10)).await
}

async fn test_reader_with_block_time(
    ethereum_rpc: String,
    ethereum_beacon_rpc: String,
    beacon_block_time: Duration,
) -> ConsensusLayerBlobReader {
    ConsensusLayerBlobReader {
        provider: ProviderBuilder::default()
            .connect(&ethereum_rpc)
            .await
            .expect("test reader should connect to ethereum rpc"),
        http_client: Client::new(),
        config: ConsensusLayerConfig {
            ethereum_rpc,
            ethereum_beacon_rpc,
            beacon_block_time,
            attempts: ATTEMPTS,
        },
    }
}

/// We had a lot of problems in the past with Consensus Layer Blob Reader: bad data arrives, retry didn't work,
/// we forgot to set valid to false on bad code and so on.
///
/// This function mimics the beacon node behaviour for testing purposes.
///
/// In practice you can send arbitrary amount of `responses` and  this function will send them in order.
async fn run_beacon_server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test beacon server should bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let paths = Arc::new(Mutex::new(Vec::new()));
    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));

    tokio::spawn({
        let paths = paths.clone();
        let responses = responses.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let paths = paths.clone();
                let responses = responses.clone();
                tokio::spawn(async move {
                    let mut buf = [0; 2048];
                    let Ok(n) = socket.read(&mut buf).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or_default()
                        .to_string();
                    paths.lock().await.push(path);

                    let body = responses
                        .lock()
                        .await
                        .pop_front()
                        .unwrap_or_else(|| r#"{"data":[]}"#.to_string());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    (url, paths)
}

async fn unused_local_http_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused local port should bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

async fn expect_blob_loaded(loader: &mut BlobLoader<Database>) -> CodeAndIdUnchecked {
    match timeout(Duration::from_secs(2), loader.next())
        .await
        .expect("loader must emit before timeout")
        .expect("loader stream should yield an event")
        .expect("loader event should be ok")
    {
        BlobLoaderEvent::BlobLoaded(code_and_id) => code_and_id,
    }
}

async fn run_anvil_blob_loader_test(code: Vec<u8>) {
    let signer = Signer::memory();
    let private_key: PrivateKey =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
    let public_key = signer.import(private_key).unwrap();
    let alice_address = signer.address(public_key);

    let beacon_block_time = Duration::from_secs(1);
    let anvil = Anvil::new().block_time(beacon_block_time.as_secs()).spawn();

    let ethereum = EthereumDeployer::new(&anvil.ws_endpoint(), signer.clone(), alice_address)
        .await
        .unwrap()
        .with_validators(vec![alice_address].try_into().unwrap())
        .deploy()
        .await
        .unwrap();

    let consensus_cfg = ConsensusLayerConfig {
        ethereum_rpc: anvil.endpoint(),
        ethereum_beacon_rpc: anvil.endpoint(),
        beacon_block_time,
        attempts: ATTEMPTS,
    };

    let (tx_hash, code_id) = ethereum
        .router()
        .request_code_validation(&code)
        .await
        .unwrap();

    let db = Database::memory();
    set_blob_info(&db, code_id, tx_hash);

    let mut loader = BlobLoader::new(db, consensus_cfg)
        .await
        .expect("blob loader should connect to anvil");
    loader
        .load_codes(HashSet::from([code_id]))
        .expect("CodeBlobInfo was inserted");

    let loaded = expect_blob_loaded(&mut loader).await;
    assert_eq!(loaded.code_id, code_id);
    assert_eq!(loaded.code, code);
}

async fn request_code_validation(
    chain_id: u64,
    beacon_block_time: Duration,
    code: &[u8],
) -> (alloy::node_bindings::AnvilInstance, H256, CodeId) {
    let signer = Signer::memory();
    let private_key: PrivateKey =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
    let public_key = signer.import(private_key).unwrap();
    let alice_address = signer.address(public_key);
    let anvil = Anvil::new()
        .chain_id(chain_id)
        .block_time(beacon_block_time.as_secs())
        .spawn();

    let ethereum = EthereumDeployer::new(&anvil.ws_endpoint(), signer.clone(), alice_address)
        .await
        .unwrap()
        .with_validators(vec![alice_address].try_into().unwrap())
        .deploy()
        .await
        .unwrap();

    let (tx_hash, code_id) = ethereum
        .router()
        .request_code_validation(code)
        .await
        .unwrap();

    (anvil, tx_hash, code_id)
}

#[tokio::test]
async fn load_codes_fails_when_code_blob_info_is_missing() {
    let anvil = Anvil::new().spawn();

    let db = Database::memory();
    let reader = test_reader(anvil.endpoint(), anvil.endpoint()).await;
    let mut loader = BlobLoader::new_with_consensus_reader(db, reader);
    let code_id = CodeId::generate(&[1, 2, 3, 4]);

    let err = loader
        .load_codes(HashSet::from([code_id]))
        .expect_err("missing CodeBlobInfo must fail");

    assert!(matches!(err, BlobLoaderError::CodeBlobInfoNotFound(id) if id == code_id));
    assert_eq!(loader.pending_codes_len(), 0);
}

#[tokio::test]
async fn already_loaded_code_is_emitted_without_remote_read() {
    let anvil = Anvil::new().spawn();

    let db = Database::memory();
    let code = generated_code(64);
    let code_id = db.set_original_code(&code);
    let tx_hash = H256::random();
    set_blob_info(&db, code_id, tx_hash);

    let reader = test_reader(anvil.endpoint(), unused_local_http_url().await).await;
    let mut loader = BlobLoader::new_with_consensus_reader(db, reader);

    loader
        .load_codes(HashSet::from([code_id]))
        .expect("metadata exists");

    assert_eq!(loader.pending_codes_len(), 1);
    let loaded = expect_blob_loaded(&mut loader).await;

    assert_eq!(loaded.code_id, code_id);
    assert_eq!(loaded.code, code);
    assert_eq!(loader.pending_codes_len(), 0);
}

#[tokio::test]
async fn reader_failure_does_not_emit_success_or_terminate_stream() {
    let anvil = Anvil::new().spawn();

    let db = Database::memory();
    let code = generated_code(128);
    let code_id = CodeId::generate(&code);
    let tx_hash = H256::random();
    set_blob_info(&db, code_id, tx_hash);

    let reader = test_reader(anvil.endpoint(), anvil.endpoint()).await;
    let mut loader = BlobLoader::new_with_consensus_reader(db, reader);

    loader
        .load_codes(HashSet::from([code_id]))
        .expect("metadata exists");

    let no_event = timeout(Duration::from_millis(100), loader.next()).await;

    assert!(
        no_event.is_err(),
        "reader failure should be logged and skipped, not emitted as success"
    );
    assert!(!loader.is_terminated());
}

#[tokio::test]
async fn repeated_load_codes_for_pending_code_schedules_one_remote_read() {
    let code = generated_code(128);
    let code_id = CodeId::generate(&code);
    let tx_hash = H256::random();

    let db = Database::memory();
    set_blob_info(&db, code_id, tx_hash);

    let reader = ConsensusLayerBlobReader {
        provider: ProviderBuilder::default().connect_http("http://127.0.0.1:1".parse().unwrap()),
        http_client: Client::new(),
        config: ConsensusLayerConfig {
            ethereum_rpc: String::new(),
            ethereum_beacon_rpc: String::new(),
            beacon_block_time: Duration::from_secs(1),
            attempts: ATTEMPTS,
        },
    };
    let mut loader = BlobLoader::new_with_consensus_reader(db, reader);

    loader
        .load_codes(HashSet::from([code_id]))
        .expect("first request should be accepted");
    loader
        .load_codes(HashSet::from([code_id]))
        .expect("duplicate pending request should be ignored");

    assert_eq!(loader.pending_codes_len(), 1);
    assert_eq!(loader.futures.len(), 1);
}

#[tokio::test]
async fn blob_loader_reads_code_from_anvil_tx() {
    run_anvil_blob_loader_test(generated_code(128)).await;
}

#[tokio::test]
async fn blob_loader_reads_code_larger_than_three_blob_chunks_from_anvil_tx() {
    run_anvil_blob_loader_test(generated_code(3 * BLOB_CHUNK_SIZE + 17)).await;
}

#[tokio::test]
async fn consensus_reader_reports_beacon_rpc_disconnect() {
    let anvil = Anvil::new().spawn();
    let reader = test_reader(anvil.endpoint(), unused_local_http_url().await).await;

    let err = reader
        .read_blob_bundle(0, &[B256::ZERO])
        .await
        .expect_err("disconnected beacon rpc should fail");

    assert!(matches!(err, ReadBlobBundleError::Reqwest(_)));
}

#[tokio::test]
async fn consensus_reader_uses_beacon_genesis_slot_for_non_anvil_chain_id() {
    let beacon_block_time = Duration::from_secs(1);
    let code = generated_code(128);
    let (anvil, tx_hash, code_id) = request_code_validation(1, beacon_block_time, &code).await;
    let provider: RootProvider = ProviderBuilder::default()
        .connect(&anvil.endpoint())
        .await
        .unwrap();
    let tx = provider
        .get_transaction_by_hash(tx_hash.0.into())
        .await
        .unwrap()
        .unwrap();
    let block_hash = tx.block_hash.unwrap();
    let block = provider
        .get_block_by_hash(block_hash)
        .await
        .unwrap()
        .unwrap();
    let expected_slot = block.header.number;
    let genesis_time = block.header.timestamp - expected_slot;
    let blob_body = reqwest::get(format!(
        "{}/eth/v1/beacon/blobs/{expected_slot}?versioned_hashes={}",
        anvil.endpoint(),
        tx.blob_versioned_hashes().unwrap()[0]
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    let (beacon_rpc, paths) = run_beacon_server(vec![
            format!(
                r#"{{"data":{{"genesis_time":"{genesis_time}","genesis_validators_root":"0x0000000000000000000000000000000000000000000000000000000000000000","genesis_fork_version":"0x00000000"}}}}"#
            ),
            blob_body,
        ])
        .await;
    let reader = test_reader_with_block_time(anvil.endpoint(), beacon_rpc, beacon_block_time).await;

    let blob = reader.read_blob(code_id, tx_hash).await.unwrap();

    assert_eq!(blob, code);
    let paths = paths.lock().await;
    assert!(paths.iter().any(|path| path == "/eth/v1/beacon/genesis"));
    assert!(paths.iter().any(|path| {
        path.starts_with(&format!(
            "/eth/v1/beacon/blobs/{expected_slot}?versioned_hashes="
        ))
    }));
}

#[tokio::test]
async fn consensus_reader_re_requests_blob_after_transient_invalid_json() {
    let beacon_block_time = Duration::from_secs(1);
    let code = generated_code(128);
    let (anvil, tx_hash, code_id) = request_code_validation(31337, beacon_block_time, &code).await;
    let provider: RootProvider = ProviderBuilder::default()
        .connect(&anvil.endpoint())
        .await
        .unwrap();
    let tx = provider
        .get_transaction_by_hash(tx_hash.0.into())
        .await
        .unwrap()
        .unwrap();
    let slot = tx.block_number.unwrap();
    let blob_body = reqwest::get(format!(
        "{}/eth/v1/beacon/blobs/{slot}?versioned_hashes={}",
        anvil.endpoint(),
        tx.blob_versioned_hashes().unwrap()[0]
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    let (beacon_rpc, paths) = run_beacon_server(vec!["not json".to_string(), blob_body]).await;
    let reader = test_reader(anvil.endpoint(), beacon_rpc).await;

    let blob = reader.read_blob(code_id, tx_hash).await.unwrap();

    assert_eq!(blob, code);
    let blob_requests = paths
        .lock()
        .await
        .iter()
        .filter(|path| path.starts_with(&format!("/eth/v1/beacon/blobs/{slot}")))
        .count();
    assert_eq!(blob_requests, 2);
}

#[test]
fn test_handle_blob() {
    let code_id = CodeId::generate(&[1, 2, 3, 4]);

    // correct blob
    let blob = vec![1, 2, 3, 4];
    let mut previously_received_code_id = None;
    let result = handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 1).unwrap();
    assert_eq!(result, blob);

    // blob with incorrect code id
    let blob = vec![4, 3, 2, 1];
    let blob_code_id = CodeId::generate(&blob);
    let mut previously_received_code_id = None;
    let result = handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 1);
    assert!(matches!(
        result,
        Err(ReaderError::CodeIdMismatch {
            expected,
            found,
        }) if expected == code_id && found == blob_code_id
    ),);
    assert_eq!(previously_received_code_id, Some(blob_code_id));

    // same incorrect blob again - should be considered as loaded
    let result = handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 2).unwrap();
    assert_eq!(result, blob);

    // same incorrect blob again, but another code id
    let previously_received_code_id = CodeId::from([1; 32]);
    let result = handle_blob(
        blob.clone(),
        code_id,
        &mut Some(previously_received_code_id),
        2,
    );
    assert!(matches!(
        result,
        Err(ReaderError::CodeIdMismatch {
            expected,
            found,
        }) if expected == code_id && found == blob_code_id
    ));

    // empty blob
    let blob = vec![];
    let mut previously_received_code_id = None;
    let result = handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 1);
    assert!(result.is_err());
    assert!(previously_received_code_id.is_none());
}
