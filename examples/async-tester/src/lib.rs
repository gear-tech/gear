#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    include! {"./code.rs"}
}

#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq)]
pub enum Kind {
    Send,
    SendWithGas(u64),
    SendBytes,
    SendBytesWithGas(u64),
    SendCommit,
    SendCommitWithGas(u64),
}
