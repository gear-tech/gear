//! WASM execution external state
use wasmtime::{AsContextMut, Memory, Trap};

/// External state
#[derive(Default)]
pub struct Ext {
    pub msg: Vec<u8>,
    pub timestamp: u64,
    pub height: u64,
}
