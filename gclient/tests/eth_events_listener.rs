//! Example of querying logs from the Ethereum network.

use alloy::{
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::Filter, sol_types::SolEvent,
};
use eyre::{Report, Result};
use demo_ethereum_bridge_common::ETH_BRIDGE::EthToVaraTransferEvent;
use demo_ethereum_common::{ethereum_types::{Address, Bloom, H256, U256}, patricia_trie::{TrieDB, TrieDBMut}, rlp::RlpStream, trie_db::{Recorder, Trie, TrieMut}, types::{self, Bytes, Log, Receipt}};
use circular_buffer::CircularBuffer;
use serde_json::json;
use std::time::Duration;
use serde::Deserialize;
use tokio::time;
use gclient::{EventListener, EventProcessor, GearApi};
use gstd::prelude::*;
use demo_ethereum_bridge::{WASM_BINARY, Init, EthToVaraEvent};
use demo_fungible_token::WASM_BINARY as FT_BINARY;
use ft_io::InitConfig as FtInitConfig;

pub const REQWEST_TIMEOUT_TIME: u64 = 5;
pub const HASH_LENGTH: usize = 32;
const ENDPOINT: &str = "https://eth-sepolia.g.alchemy.com/v2/_YDfAxTPJpWTXrxR8a_7NpGQUWW6R0aB";

type BlockReceipt = (U256, Receipt);

const LIGHT_CLIENT: [u8; 32] = [75, 71, 158, 8, 27, 93, 195, 239, 189, 11, 224, 76, 225, 92, 164, 242, 242, 3, 223, 169, 90, 116, 97, 23, 19, 154, 80, 98, 20, 251, 86, 26];

async fn common_upload_program(
    client: &GearApi,
    code: Vec<u8>,
    payload: impl Encode,
) -> Result<([u8; 32], [u8; 32])> {
    let encoded_payload = payload.encode();
    let gas_limit = client
        .calculate_upload_gas(None, code.clone(), encoded_payload, 0, true)
        .await?
        .min_limit;
    println!("init gas {gas_limit:?}");
    let (message_id, program_id, _) = client
        .upload_program(
            code,
            gclient::now_micros().to_le_bytes(),
            payload,
            gas_limit,
            0,
        )
        .await?;

    Ok((message_id.into(), program_id.into()))
}

async fn upload_program(
    client: &GearApi,
    listener: &mut EventListener,
    payload: impl Encode,
    binary: Vec<u8>,
) -> Result<[u8; 32]> {
    let (message_id, program_id) =
        common_upload_program(client, binary, payload).await?;

    assert!(listener
        .message_processed(message_id.into())
        .await?
        .succeed());

    Ok(program_id)
}

#[tokio::test]
async fn eth_events_listener() -> Result<()> {
    let client = GearApi::dev().await?;
    let mut listener = client.subscribe().await?;

    let fungible_token_id = upload_program(
        &client,
        &mut listener,
        FtInitConfig {
            name: "Vara WrappedETH".to_string(),
            symbol: "vwETH".to_string(),
            decimals: 18,
            initial_capacity: None,
        },
        FT_BINARY.to_vec(),
    )
    .await?;

    println!("111");

    let program_id = upload_program(
        &client,
        &mut listener,
        Init {
            light_client: LIGHT_CLIENT,
            fungible_token: fungible_token_id,
        },
        WASM_BINARY.to_vec(),
    )
    .await?;

    println!("222");

    // Create a provider.
    let rpc_url = ENDPOINT.parse()?;
    let provider = ProviderBuilder::new().on_http(rpc_url);

    let filter = Filter::new()
        .event_signature(EthToVaraTransferEvent::SIGNATURE_HASH)
        // .from_block(5_920_000);
        .from_block(5_919_892)
        .to_block(5_919_900);
        // .to_block(5_920_000);
    // You could also use the event name instead of the event signature like so:
    // .event("Transfer(address,address,uint256)")

    // Get all logs from the latest block that match the filter.
    let logs = provider.get_logs(&filter).await?;

    // Subscribe to logs.
    // let sub = provider.subscribe_logs(&filter).await?;
    // let mut stream = sub.into_stream();

    let mut buf = CircularBuffer::<256, (u64, Vec<BlockReceipt>)>::new();
    for log in logs {
    // while let Some(log) = stream.next().await {
        println!("Transfer event: {log:?}");
        println!();

        let Some(block_number) = log.block_number else {
            println!("unable to get block number from log");
            continue;
        };

        let Some(transaction_index) = log.transaction_index else {
            println!("unable to get transaction index from log");
            continue;
        };

        match buf.front() {
            Some((block_number_first, _)) if block_number < *block_number_first => {
                println!("log is not relevant");
                continue;
            }

            _ => ()
        }

        let receipts = match buf.iter().find(|(block_number_cached, _receipts)| *block_number_cached == block_number) {
            Some((_, receipts)) => receipts,
            _ => {
                let receipts = get_block_receipts(ENDPOINT, block_number).await?;
                buf.push_back((block_number, receipts));

                buf.back().map(|(_, receipts)| receipts).expect("the item has been just pushed")
            }
        };

        let log_receipt = receipts
            .iter()
            .find(|(index, _)| index == &U256::from(transaction_index));
        let Some((transaction_index_u256, log_receipt)) = log_receipt else {
            println!("unable to find the log's receipt");
            continue;
        };

        let (key, expected_value) = get_rlp_encoded_receipt_and_encoded_key_tuple(transaction_index_u256, &log_receipt);
        let key_value_tuples = get_rlp_encoded_receipts_and_nibble_tuples(&receipts[..]);
        let mut memdb = demo_ethereum_common::new_memory_db();
        let root = {
            let mut root = H256::zero();
            let mut triedbmut = TrieDBMut::new(&mut memdb, &mut root);
            for (key, value) in &key_value_tuples {
                triedbmut.insert(&key, &value).unwrap();
            }

            *triedbmut.root()
        };

        let trie = TrieDB::new(&memdb, &root).unwrap();
        // construct proof
        let mut recorder = Recorder::new();
        let value = trie.get_with(&key, &mut recorder);

        assert!(
            matches!(
                value, Ok(Some(value)) if expected_value == value,
            )
        );

        // for node in recorder.drain() {
        //     println!("recorded node data: {:?}", hex::encode(&node.data));
        // }

        println!();
        println!();

        let payload = EthToVaraEvent {
            block_number,
            transaction_index,
            proof: recorder.drain().into_iter().map(|r| r.data).collect(),
            receipt: log_receipt.clone(),
        };
        let gas_limit = client
            .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
            .await?
            .min_limit;
        println!("gas_limit {gas_limit:?}");
    
        let (message_id, _) = client
            .send_message(program_id.into(), payload, gas_limit, 0)
            .await?;
    
        assert!(listener.message_processed(message_id).await?.succeed());

        time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

async fn get_block_receipts(endpoint: &str, block_number: u64) -> Result<Vec<BlockReceipt>> {
    let block_number = format!("{block_number:#x}");
    let request = json!({
        "id": "1",
        "jsonrpc": "2.0",
        "method": "eth_getBlockReceipts",
        "params": [ block_number ],
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQWEST_TIMEOUT_TIME))
        .build()?;
    let response = client.post(endpoint).json(&request).send().await?;

    let res_text = response.text().await?;
    let res_text = match res_text.contains("error") {
        true => return Err(Report::msg(format!(
            "✘ RPC call failed!\n✘ {res_text}",
        ))),

        false => match res_text.contains("\"result\":null") {
            true => return Err(Report::msg(
                "✘ No receipt found for that transaction hash!"
            )),
            false => res_text,
        },
    };

    let json_block_receipts: BlockReceiptsRpcResponse = serde_json::from_str(&res_text)?;
    json_block_receipts
        .result
        .into_iter()
        .map(|json_receipt| deserialize_receipt_json_to_receipt_struct(json_receipt))
        .collect::<Result<Vec<_>>>()
}

#[derive(Debug, Deserialize)]
pub struct BlockReceiptsRpcResponse {
    pub result: Vec<ReceiptJson>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
pub struct ReceiptJson {
    pub from: String,
    pub status: String,
    pub gasUsed: String,
    pub blockHash: String,
    pub logsBloom: String,
    pub logs: Vec<LogJson>,
    pub blockNumber: String,
    pub to: serde_json::Value,
    pub transactionHash: String,
    pub transactionIndex: String,
    pub cumulativeGasUsed: String,
    pub contractAddress: serde_json::Value,
    pub r#type: String,
}

#[allow(non_snake_case)]
#[derive(Clone, Debug, Deserialize)]
pub struct LogJson {
    pub data: String,
    pub removed: bool,
    // pub r#type: String,
    pub address: String,
    pub logIndex: String,
    pub blockHash: String,
    pub blockNumber: String,
    pub topics: Vec<String>,
    pub transactionHash: String,
    pub transactionIndex: String,
}

pub fn deserialize_receipt_json_to_receipt_struct(receipt: ReceiptJson) -> Result<BlockReceipt> {
    // info!("type = {:?}", &receipt.r#type);
    let r#type = receipt.r#type.trim();
    let r#type = if r#type.starts_with("0x") {
        &r#type[2..]
    } else {
        &r#type[..]
    };

    let transaction_index = convert_hex_to_u256(receipt.transactionIndex)?;

    let logs = receipt
        .logs
        .iter()
        .map(|log_json| get_log_from_json(log_json))
        .collect::<Result<Vec<Log>>>()?;
    Ok((transaction_index, Receipt {
        logs_bloom: logs.iter().fold(Bloom::default(), |mut bloom, log| {
            bloom.accrue_bloom(&log.calculate_bloom());
            bloom
        }),
        cumulative_gas_used: convert_hex_to_u256(receipt.cumulativeGasUsed)?,
        status: match receipt.status.as_ref() {
            "0x1" => true,
            "0x0" => false,
            _ => false,
        },
        logs,
        r#type: u64::from_str_radix(r#type, 16)
            .map_err(|e| Report::msg(e.to_string()))?,
    }))
}

fn get_log_from_json(log_json: &LogJson) -> Result<Log> {
    Ok(Log {
        address: convert_hex_to_address(log_json.address.clone())?,
        topics: convert_hex_strings_to_h256s(log_json.topics.clone())?,
        data: convert_hex_to_bytes(log_json.data.clone())?,
    })
}

pub fn decode_hex(hex_to_decode: String) -> Result<Vec<u8>> {
    Ok(hex::decode(hex_to_decode)?)
}

pub fn convert_hex_to_address(hex: String) -> Result<Address> {
    decode_prefixed_hex(hex).map(|bytes| Address::from_slice(&bytes))
}

pub fn strip_hex_prefix(prefixed_hex: &str) -> Result<String> {
    let res = str::replace(prefixed_hex, "0x", "");
    match res.len() % 2 {
        0 => Ok(res),
        _ => left_pad_with_zero(&res),
    }
}

pub fn decode_prefixed_hex(hex_to_decode: String) -> Result<Vec<u8>> {
    strip_hex_prefix(&hex_to_decode).and_then(decode_hex)
}

fn left_pad_with_zero(string: &str) -> Result<String> {
    Ok(format!("0{}", string))
}

pub fn convert_hex_to_bytes(hex: String) -> Result<Bytes> {
    Ok(hex::decode(strip_hex_prefix(&hex)?)?)
}

pub fn convert_hex_to_h256(hex: String) -> Result<H256> {
    decode_prefixed_hex(hex).and_then(|bytes| match bytes.len() {
        HASH_LENGTH => Ok(H256::from_slice(&bytes)),
        _ => Err(Report::msg(
            "✘ Wrong number of bytes in hex to create H256 type!",
        )),
    })
}

pub fn convert_hex_strings_to_h256s(hex_strings: Vec<String>) -> Result<Vec<H256>> {
    hex_strings.into_iter().map(convert_hex_to_h256).collect()
}

pub fn convert_hex_to_u256(hex: String) -> Result<U256> {
    decode_prefixed_hex(hex).map(|ref bytes| U256::from_big_endian(bytes))
}

pub fn get_rlp_encoded_receipts_and_nibble_tuples(
    receipts: &[BlockReceipt],
) -> Vec<(Bytes, Bytes)> {
    receipts
        .iter()
        .map(|(transaction_index, receipt)| get_rlp_encoded_receipt_and_encoded_key_tuple(transaction_index, receipt))
        .collect::<Vec<_>>()
}

pub fn rlp_encode_transaction_index(index: &U256) -> Bytes {
    let mut rlp_stream = RlpStream::new();
    rlp_stream.append(&index.as_usize());
    rlp_stream.out().to_vec()
}

pub fn get_rlp_encoded_receipt_and_encoded_key_tuple(
    index: &U256,
    receipt: &Receipt,
) -> (Bytes, Bytes) {
    (rlp_encode_transaction_index(index), types::rlp_encode_receipt(receipt))
}
