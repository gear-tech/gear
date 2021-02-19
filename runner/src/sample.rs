use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Program {
    pub path: String,
    pub id: u64,
    pub init_message: Option<Message>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expectation {
    pub step: u64,
    pub messages: Vec<Message>,
    pub allocation: Vec<AllocationStorage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fixture {
    pub title: String,
    pub messages: Vec<Message>,
    pub expected: Expectation,
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
}

impl PayloadVariant {
    pub fn raw(&self) -> &[u8] {
        match *self {
            Self::Utf8(ref s) => s.as_bytes(),
            Self::Int32(ref s) => s.as_ne_bytes(),
            Self::Int64(ref s) => s.as_ne_bytes(),
            Self::Float32(ref s) => s.as_ne_bytes(),
            Self::Float64(ref s) => s.as_ne_bytes(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AllocationStorage {
    pub page_num: u32,
    pub program_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Message {
    pub destination: u64,
    pub payload: PayloadVariant,
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
                        "destination": 1
                    }
                ],
                "expected": {
                    "messages": [
                        {
                            "payload": { "kind": "utf-8", "value": "PING" },
                            "destination": 0
                        }
                    ]
                }
            }
        ]
    }
    "#;

    let test: Test = serde_json::from_str(json).unwrap();

    assert_eq!(test.fixtures[0].messages.len(), 1);
    assert_eq!(test.fixtures[0].messages.len(), 1);
}
