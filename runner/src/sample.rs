use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Program {
    pub path: String,
    pub id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expectation {
    messages: Vec<Message>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fixture {
    pub title: String,
    pub messages: Vec<Message>,
    pub expected: Expectation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag="kind", content="value")]
pub enum PayloadVariant{
    #[serde(rename="utf-8")]
    Utf8(String),
}

impl PayloadVariant {
    pub fn raw(&self) -> &[u8] {
        match *self {
            Self::Utf8(ref s) => s.as_bytes(),
        }
    }
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
