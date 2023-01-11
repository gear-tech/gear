#![cfg(test)]

use crate::metadata::{registry::LocalRegistry, Metadata};
use parity_scale_codec::{Decode, Encode};
use scale_info::{
    form::Form, IntoPortable, MetaType, PortableRegistry, Registry, TypeDef, TypeInfo,
};

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

#[test]
fn test_parsing_metadata() {
    let demo_meta = demo_meta::WASM_BINARY_META;
    let metadata = Metadata::of(demo_meta).expect("get metadata failed");

    assert_eq!(
        metadata,
        Metadata {
            meta_title: Some("Example program with metadata".into()),
            meta_init_input: Some("MessageInitIn".into()),
            meta_init_output: Some("MessageInitOut".into()),
            meta_async_init_input: Some("MessageInitAsyncIn".into()),
            meta_async_init_output: Some("MessageInitAsyncOut".into()),
            meta_handle_input: Some("MessageIn".into()),
            meta_handle_output: Some("MessageOut".into()),
            meta_async_handle_input: Some("MessageHandleAsyncIn".into()),
            meta_async_handle_output: Some("MessageHandleAsyncOut".into()),
            meta_state_input: Some("Option<Id>".into()),
            meta_state_output: Some("Vec<Wallet>".into()),
            meta_registry: None
        }
    );

    assert_eq!(
        DEMO_METADATA.trim(),
        &format!("{metadata:#}").replace('"', "")
    );
}

// # Note
//
// tests below are reserved for constructing metadata types in pure rust

#[test]
#[ignore]
fn test_encode_depth1_1() {
    /// Depth 1 with 1 parameter
    #[derive(Encode, Decode)]
    struct Depth1_1 {
        number: u32,
    }

    let depth_1_1 = Depth1_1 { number: 42 };
    let encoded = depth_1_1.encode();
    assert_eq!(encoded, (42).encode());
}

#[test]
#[ignore]
fn test_encode_depth1_2() {
    /// Depth 1 with 2 parameters
    #[derive(Encode, Decode)]
    struct Depth1_2 {
        foo: u32,
        bar: u32,
    }

    let depth1_2 = Depth1_2 { foo: 42, bar: 42 };
    let encoded = depth1_2.encode();
    assert_eq!(encoded, (42, 42).encode());
}

#[test]
#[ignore]
fn test_encode_depth2_2() {
    // Depth 1 with 2 paramters
    #[derive(Encode, Decode)]
    struct Depth1_2 {
        foo: u32,
        bar: u32,
    }

    // Depth 2 with 2 parameters
    #[derive(Encode, Decode)]
    struct Depth2_2 {
        foo: Depth1_2,
        bar: u32,
    }

    let depth1_2 = Depth1_2 { foo: 42, bar: 42 };
    let depth2_2 = Depth2_2 {
        foo: depth1_2,
        bar: 42,
    };
    let encoded = depth2_2.encode();
    assert_eq!(encoded, ((42, 42), 42).encode());
    assert_eq!(encoded, (42, 42, 42).encode());
}
