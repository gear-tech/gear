//! Tests for metadata
use crate::meta::Meta;

const METADATA: &str = r#"
Metadata {
    init:  {
        input: MessageInitIn {
            amount: "u8",
            currency: "String",
        },
        output: MessageInitOut {
            exchange_rate: "Result<u8, u8>",
            sum: "u8",
        },
    },
    handle:  {
        input: MessageIn {
            id: "Id",
        },
        output: MessageOut {
            res: "Option<Wallet>",
        },
    },
    reply:  {
        input: str,
        output: [u16],
    },
    others:  {
        input: MessageAsyncIn {
            empty: "()",
        },
        output: Option ,
    },
    signal: "()",
    state: [Wallet { id: "Id", person: "Person" }],
}
"#;

#[test]
fn test_parse_metadata_works() {
    use demo_new_meta::WASM_METADATA;
    let meta = Meta::decode(&WASM_METADATA).expect("Failed to decode wasm metadata");
    assert_eq!(format!("{:#}", meta), METADATA.trim());
}
