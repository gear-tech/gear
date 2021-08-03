use hex::FromHex;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;

fn de_address<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => {
            let without_prefix = s.trim_start_matches("0x");
            usize::from_str_radix(without_prefix, 16).map_err(de::Error::custom)?
        }
        _ => return Err(de::Error::custom("wrong type")),
    })
}

fn de_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => {
            let without_prefix = s.trim_start_matches("0x");
            Vec::from_hex(without_prefix).map_err(de::Error::custom)?
        }
        _ => return Err(de::Error::custom("wrong type")),
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Program {
    pub path: String,
    pub id: u64,
    pub init_message: Option<PayloadVariant>,
    pub init_gas_limit: Option<u64>,
    pub init_value: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expectation {
    pub step: Option<u64>,
    pub messages: Option<Vec<Message>>,
    pub allocations: Option<Vec<AllocationStorage>>,
    pub memory: Option<Vec<BytesAt>>,
    pub log: Option<Vec<Message>>,
    #[serde(rename = "allowError")]
    pub allow_error: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fixture {
    pub title: String,
    pub messages: Vec<Message>,
    pub expected: Vec<Expectation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum PayloadVariant {
    #[serde(rename = "utf-8")]
    Utf8(String),
    #[serde(rename = "i32")]
    Int32(i32),
    #[serde(rename = "i64")]
    Int64(i64),
    #[serde(rename = "f32")]
    Float32(f32),
    #[serde(rename = "f64")]
    Float64(f64),
    #[serde(rename = "bytes", deserialize_with = "de_bytes")]
    Bytes(Vec<u8>),
}

impl Default for PayloadVariant {
    fn default() -> Self {
        Self::Bytes(Vec::new())
    }
}

impl PayloadVariant {
    pub fn into_raw(self) -> Vec<u8> {
        match self {
            Self::Utf8(v) => v.into_bytes(),
            Self::Int32(v) => v.to_le_bytes().to_vec(),
            Self::Int64(v) => v.to_le_bytes().to_vec(),
            Self::Float32(v) => v.to_le_bytes().to_vec(),
            Self::Float64(v) => v.to_le_bytes().to_vec(),
            Self::Bytes(v) => v,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.clone().into_raw()
    }

    pub fn equals(&self, val: &[u8]) -> bool {
        let bytes = self.to_bytes();
        &bytes[..] == val
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BytesAt {
    pub program_id: u64, // required for static memory
    #[serde(rename = "at")]
    #[serde(deserialize_with = "de_address")]
    pub address: usize,
    #[serde(deserialize_with = "de_bytes")]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AllocationStorage {
    pub page_num: u32,
    pub program_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Message {
    pub destination: u64,
    pub payload: Option<PayloadVariant>,
    pub gas_limit: Option<u64>,
    pub value: Option<u64>,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Test {
    pub programs: Vec<Program>,
    pub fixtures: Vec<Fixture>,
}

#[test]
fn check_sample() {
    let json = r#"{
        "title": "basic",
        "programs": [
            {
                "id": 1,
                "path": "../../demo-chat/target/wasm32-unknown-unknown/release/demo1.wasm"
            }
        ],
        "fixtures": [
            {
                "title": "ping-pong",
                "messages": [
                    {
                        "payload": { "kind": "utf-8", "value": "PING" },
                        "destination": 1,
                        "gas_limit": 1000000
                    }
                ],
                "expected": [
                    {
                        "messages": [
                            {
                                "payload": { "kind": "utf-8", "value": "PING" },
                                "destination": 0
                            },
                            {
                                "destination": 2
                            }
                        ],
                        "allocations": [
                            {
                                "page_num": 256,
                                "program_id": 1
                            }
                        ],
                         "memory": [
                            {
                                "program_id": 1,
                                "at": "0x100038",
                                "bytes": "0x54455354"
                            }
                        ]
                    }
                ]
            }
        ]
    }
    "#;

    let test: Test = serde_json::from_str(json).unwrap();

    assert_eq!(test.fixtures[0].messages.len(), 1);
    assert_eq!(test.fixtures[0].messages.len(), 1);
}
