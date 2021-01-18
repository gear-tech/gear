use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Program {
    path: String,
    id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Expectation {
    messages: Vec<Message>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Fixture {
    title: String,
    messages: Vec<Message>,
    expected: Expectation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag="kind", content="value")]
enum PayloadVariant{
    #[serde(rename="utf-8")]
    Utf8(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Message {
    destination: u64,
    payload: PayloadVariant,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Test {
    programs: Vec<Program>,
    fixtures: Vec<Fixture>,
}

pub struct InitialiState {
    messages: 
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
