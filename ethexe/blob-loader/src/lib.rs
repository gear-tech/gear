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

use alloy::{
    consensus::{SidecarCoder, SimpleCoder, Transaction},
    primitives::B256,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::beacon::{genesis::GenesisResponse, sidecar::GetBlobsResponse},
    transports::{RpcError, TransportErrorKind},
};
use ethexe_common::{
    CodeAndIdUnchecked, CodeBlobInfo,
    db::{CodesStorageRO, OnChainStorageRO},
    gear_core::ids::prelude::CodeIdExt,
};
use futures::{
    FutureExt, Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
};
use gprimitives::{CodeId, H256};
use reqwest::Client;
use std::{collections::HashSet, fmt, num::NonZero, pin::Pin, task::Poll};
use tokio::{sync::OnceCell, time::Duration};

#[derive(Clone, PartialEq, Eq)]
pub enum BlobLoaderEvent {
    BlobLoaded(CodeAndIdUnchecked),
}

#[derive(thiserror::Error, Debug)]
pub enum BlobLoaderError {
    #[error("transport error: {0}")]
    Transport(#[from] RpcError<TransportErrorKind>),
    #[error("failed to get code blob info for: {0}")]
    CodeBlobInfoNotFound(CodeId),
}

#[derive(thiserror::Error, Debug)]
enum ReadBlobBundleError {
    #[error("failed to read blob bundle from beacon node: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("failed to decode blob bundle response: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(thiserror::Error, Debug)]
enum ReaderError {
    #[error("transport error: {0}")]
    Transport(#[from] RpcError<TransportErrorKind>),
    #[error("failed to find transaction by hash: {0}")]
    TransactionNotFound(H256),
    #[error("failed to get blob versioned hashes from transaction: {0}")]
    BlobVersionedHashesNotFound(H256),
    #[error("failed to get transaction block hash: {0}")]
    TransactionBlockNotFound(H256),
    #[error("failed to get block by hash: {0}")]
    BlockNotFound(H256),
    #[error("failed to read blob bundle: {0}")]
    ReadBlob(#[from] ReadBlobBundleError),
    #[error("failed to decode blobs")]
    DecodeBlobs,
    #[error("failed to access genesis time: {0}")]
    GenesisTimeAccess(reqwest::Error),
    #[error("received empty blob")]
    EmptyBlob,
    #[error("blob code id mismatch: expected {expected:?}, found {found:?}")]
    CodeIdMismatch { expected: CodeId, found: CodeId },
}

type LoaderResult<T> = Result<T, BlobLoaderError>;
type ReaderResult<T> = Result<T, ReaderError>;

pub trait BlobLoaderService:
    Stream<Item = LoaderResult<BlobLoaderEvent>> + FusedStream + Send + Unpin
{
    fn load_codes(&mut self, codes: HashSet<CodeId>) -> LoaderResult<()>;

    fn into_box(self) -> Box<dyn BlobLoaderService>;

    fn pending_codes_len(&self) -> usize;
}

impl fmt::Debug for BlobLoaderEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlobLoaderEvent::BlobLoaded(data) => data.fmt(f),
        }
    }
}

#[derive(Clone)]
pub struct ConsensusLayerConfig {
    pub ethereum_rpc: String,
    pub ethereum_beacon_rpc: String,
    pub beacon_block_time: Duration,
    pub attempts: NonZero<u8>,
}

#[derive(Clone)]
struct ConsensusLayerBlobReader {
    provider: RootProvider,
    http_client: Client,
    config: ConsensusLayerConfig,
}

impl ConsensusLayerBlobReader {
    async fn read_blob(&self, expected_code_id: CodeId, tx_hash: H256) -> ReaderResult<Vec<u8>> {
        let mut last_err = None;
        let mut previously_received_code_id = None;
        for attempt in 0..self.config.attempts.get() {
            log::trace!("trying to get blob, attempt #{attempt}");
            match self.try_query_blob(tx_hash).await {
                Ok(blob) => {
                    match handle_blob(
                        blob,
                        expected_code_id,
                        &mut previously_received_code_id,
                        attempt,
                    ) {
                        Ok(blob) => return Ok(blob),
                        Err(err) => last_err = Some(err),
                    }
                }
                Err(err) => {
                    log::warn!("failed to get blob on attempt #{attempt}: {err}");
                    last_err = Some(err);
                }
            }

            tokio::time::sleep(self.config.beacon_block_time).await;
        }

        Err(last_err.expect("Must be set, because attempts is not zero"))
    }

    async fn try_query_blob(&self, tx_hash: H256) -> ReaderResult<Vec<u8>> {
        use ReaderError::*;

        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or(TransactionNotFound(tx_hash))?;

        // TODO: #5102 here may be a problem with code if it has same versioned hashes,
        // consider to change it to more reliable way.
        let blob_versioned_hashes = tx
            .blob_versioned_hashes()
            .ok_or(BlobVersionedHashesNotFound(tx_hash))?;

        let block_hash = tx.block_hash.ok_or(TransactionBlockNotFound(tx_hash))?;

        let block = self
            .provider
            .get_block_by_hash(block_hash)
            .await?
            .ok_or(BlockNotFound(H256(block_hash.0)))?;

        // detect anvil by chain id
        let slot = if let Some(chain_id) = tx.chain_id()
            && chain_id == 31337
        {
            block.header.number
        } else {
            static BEACON_GENESIS_BLOCK_TIME: OnceCell<u64> = OnceCell::const_new();

            let beacon_genesis_block_time = *BEACON_GENESIS_BLOCK_TIME
                .get_or_try_init(|| self.read_genesis_time())
                .await
                .map_err(GenesisTimeAccess)?;
            (block.header.timestamp - beacon_genesis_block_time)
                / self.config.beacon_block_time.as_secs()
        };

        let blob_bundle = self
            .read_blob_bundle(slot, blob_versioned_hashes)
            .await
            .map_err(ReadBlob)?;

        let mut coder = SimpleCoder::default();
        let data = coder
            .decode_all(&blob_bundle.data)
            .ok_or(DecodeBlobs)?
            .concat();

        Ok(data)
    }

    async fn read_genesis_time(&self) -> reqwest::Result<u64> {
        let ethereum_beacon_rpc = &self.config.ethereum_beacon_rpc;
        let response = self
            .http_client
            .get(format!("{ethereum_beacon_rpc}/eth/v1/beacon/genesis"))
            .send()
            .await?
            .json::<GenesisResponse>()
            .await?;

        Ok(response.data.genesis_time)
    }

    async fn read_blob_bundle(
        &self,
        slot: u64,
        versioned_hashes: &[B256],
    ) -> Result<GetBlobsResponse, ReadBlobBundleError> {
        let ethereum_beacon_rpc = &self.config.ethereum_beacon_rpc;
        let bytes = self
            .http_client
            .get(format!(
                "{ethereum_beacon_rpc}/eth/v1/beacon/blobs/{slot}?versioned_hashes={}",
                versioned_hashes
                    .iter()
                    .map(|versioned_hash| versioned_hash.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ))
            .send()
            .await?
            .bytes()
            .await?;
        let blobs_response =
            serde_json::from_slice::<GetBlobsResponse>(&bytes).inspect_err(|err| {
                log::trace!("failed to decode blob bundle response: {err}, bytes: {bytes:?}")
            })?;
        Ok(blobs_response)
    }
}

pub trait Database: CodesStorageRO + OnChainStorageRO + Unpin + Send + Clone + 'static {}
impl<T: CodesStorageRO + OnChainStorageRO + Unpin + Send + Clone + 'static> Database for T {}

pub struct BlobLoader<DB: Database> {
    futures: FuturesUnordered<BoxFuture<'static, ReaderResult<CodeAndIdUnchecked>>>,
    codes_loading: HashSet<CodeId>,

    blobs_reader: ConsensusLayerBlobReader,
    db: DB,
}

impl<DB: Database> Stream for BlobLoader<DB> {
    type Item = LoaderResult<BlobLoaderEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match futures::ready!(self.futures.poll_next_unpin(cx)) {
            None => Poll::Pending,
            Some(Err(err)) => {
                // TODO: #4995 currently in case of error in blob loading we just skip it
                log::error!("Failed to load blob: {err}, skipping");
                Poll::Pending
            }
            Some(Ok(code_and_id)) => {
                self.codes_loading.remove(&code_and_id.code_id);
                Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(code_and_id))))
            }
        }
    }
}

impl<DB: Database> FusedStream for BlobLoader<DB> {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<DB: Database> BlobLoader<DB> {
    pub async fn new(db: DB, consensus_cfg: ConsensusLayerConfig) -> LoaderResult<Self> {
        Ok(Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),

            blobs_reader: ConsensusLayerBlobReader {
                provider: ProviderBuilder::default()
                    .connect(&consensus_cfg.ethereum_rpc)
                    .await?,
                http_client: Client::new(),
                config: consensus_cfg,
            },
            db,
        })
    }

    #[cfg(test)]
    fn new_with_consensus_reader(db: DB, blobs_reader: ConsensusLayerBlobReader) -> Self {
        Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),
            blobs_reader,
            db,
        }
    }
}

impl<DB: Database> BlobLoaderService for BlobLoader<DB> {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.codes_loading.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>) -> LoaderResult<()> {
        for code_id in codes {
            if self.codes_loading.contains(&code_id) {
                continue;
            }

            let CodeBlobInfo { tx_hash, .. } = self
                .db
                .code_blob_info(code_id)
                .ok_or(BlobLoaderError::CodeBlobInfoNotFound(code_id))?;

            self.codes_loading.insert(code_id);

            if let Some(code) = self.db.original_code(code_id) {
                log::warn!("Code {code_id} is already loaded, skipping loading from remote source");
                self.futures
                    .push(futures::future::ready(Ok(CodeAndIdUnchecked { code_id, code })).boxed());
            } else {
                let blobs_reader = self.blobs_reader.clone();
                self.futures.push(
                    async move {
                        blobs_reader
                            .read_blob(code_id, tx_hash)
                            .await
                            .map(|code| CodeAndIdUnchecked { code_id, code })
                    }
                    .boxed(),
                );
            }
        }

        Ok(())
    }
}

// TODO: #4995 temporary solution to protect against inconsistent blob data,
// we have second check of code id in ethexe-processor in handle_new_code as well,
// so this solution must be reconsidered.
fn handle_blob(
    blob: Vec<u8>,
    code_id: CodeId,
    previously_received_code_id: &mut Option<CodeId>,
    attempt: u8,
) -> ReaderResult<Vec<u8>> {
    if blob.is_empty() {
        log::warn!("received empty blob on attempt #{attempt}");
        return Err(ReaderError::EmptyBlob);
    }

    let received_code_id = CodeId::generate(&blob);
    if *previously_received_code_id == Some(received_code_id) {
        log::warn!(
            "received same blob with invalid id on attempt #{attempt}, code id: {received_code_id:?}, consider blob as loaded then",
        );
        return Ok(blob);
    }

    if code_id != received_code_id {
        *previously_received_code_id = Some(received_code_id);
        log::warn!(
            "received blob code id mismatch on attempt #{attempt}: expected {code_id:?}, got {received_code_id:?}",
        );
        return Err(ReaderError::CodeIdMismatch {
            expected: code_id,
            found: received_code_id,
        });
    }

    Ok(blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use ethexe_common::{
        CodeBlobInfo,
        db::{CodesStorageRW, OnChainStorageRW},
        gear_core::ids::prelude::CodeIdExt,
    };
    use ethexe_db::Database as EthexeDatabase;
    use ethexe_ethereum::deploy::EthereumDeployer;
    use futures::{FutureExt, StreamExt};
    use gsigner::secp256k1::{PrivateKey, Signer};
    use std::{collections::VecDeque, sync::Arc};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::Mutex,
        time::{Duration, timeout},
    };

    const BLOB_CHUNK_SIZE: usize = 128 * 1024;

    fn generated_code(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn set_blob_info(db: &EthexeDatabase, code_id: CodeId, tx_hash: H256) {
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
        test_reader_with_block_time(ethereum_rpc, ethereum_beacon_rpc, Duration::from_millis(10))
            .await
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
                attempts: const { NonZero::new(3).unwrap() },
            },
        }
    }

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

    async fn expect_blob_loaded(loader: &mut BlobLoader<EthexeDatabase>) -> CodeAndIdUnchecked {
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
            attempts: const { NonZero::new(3).unwrap() },
        };

        let (tx_hash, code_id) = ethereum
            .router()
            .request_code_validation(&code)
            .await
            .unwrap();
        #[allow(unused_unsafe)]
        let db = unsafe { EthexeDatabase::memory() };
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
        #[allow(unused_unsafe)]
        let db = unsafe { EthexeDatabase::memory() };
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
        #[allow(unused_unsafe)]
        let db = unsafe { EthexeDatabase::memory() };
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
        #[allow(unused_unsafe)]
        let db = unsafe { EthexeDatabase::memory() };
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
        let beacon_block_time = Duration::from_secs(1);
        let code = generated_code(128);
        let (anvil, tx_hash, code_id) =
            request_code_validation(31337, beacon_block_time, &code).await;
        #[allow(unused_unsafe)]
        let db = unsafe { EthexeDatabase::memory() };
        set_blob_info(&db, code_id, tx_hash);

        let reader =
            test_reader_with_block_time(anvil.endpoint(), anvil.endpoint(), beacon_block_time)
                .await;
        let mut loader = BlobLoader::new_with_consensus_reader(db, reader);

        loader
            .load_codes(HashSet::from([code_id]))
            .expect("first request should be accepted");
        loader
            .load_codes(HashSet::from([code_id]))
            .expect("duplicate pending request should be ignored");

        assert_eq!(loader.pending_codes_len(), 1);
        let _ = expect_blob_loaded(&mut loader).await;
        assert!(
            loader.next().now_or_never().is_none(),
            "duplicate pending request must not queue another ready event"
        );
        assert_eq!(loader.pending_codes_len(), 0);
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
    async fn consensus_reader_reports_invalid_beacon_json() {
        let anvil = Anvil::new().spawn();
        let (beacon_rpc, _) = run_beacon_server(vec!["not json".to_string()]).await;
        let reader = test_reader(anvil.endpoint(), beacon_rpc).await;

        let err = reader
            .read_blob_bundle(0, &[B256::ZERO])
            .await
            .expect_err("invalid beacon json should fail");

        assert!(matches!(err, ReadBlobBundleError::Serde(_)));
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
        let reader =
            test_reader_with_block_time(anvil.endpoint(), beacon_rpc, beacon_block_time).await;

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
        let (anvil, tx_hash, code_id) =
            request_code_validation(31337, beacon_block_time, &code).await;
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
        let result =
            handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 1).unwrap();
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
        let result =
            handle_blob(blob.clone(), code_id, &mut previously_received_code_id, 2).unwrap();
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
}
