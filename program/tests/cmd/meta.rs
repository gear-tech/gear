//! Integration tests for command `meta`
use crate::common;

const DEMO_METADATA: &str = r#"
Metadata {
    meta_title: Example program with metadata,
    meta_init_input: MessageInitIn {
        amount: u8,
        currency: String,
    },
    meta_init_output: MessageInitOut {
        exchange_rate: Result<u8, u8>,
        sum: u8,
    },
    meta_async_init_input: MessageInitAsyncIn {
        empty: (),
    },
    meta_async_init_output: MessageInitAsyncOut {
        empty: (),
    },
    meta_handle_input: MessageIn {
        id: Id,
    },
    meta_handle_output: MessageOut {
        res: Option<Wallet>,
    },
    meta_async_handle_input: MessageHandleAsyncIn {
        empty: (),
    },
    meta_async_handle_output: MessageHandleAsyncOut {
        empty: (),
    },
    meta_state_input: Option<Id>,
    meta_state_output: Vec<Wallet>,
}
"#;

#[tokio::test]
async fn test_display_metadata_works() {
    let output =
        common::gear(&["meta", "res/demo_meta.meta.wasm", "display"]).expect("Run command failed");

    assert_eq!(
        DEMO_METADATA.trim(),
        String::from_utf8_lossy(&output.stdout).trim()
    );
}
