#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub fn system_reserve() -> u64 {
    gstd::SYSTEM_RESERVE
}

pub fn reply_duration() -> u32 {
    1
}

#[cfg(not(feature = "std"))]
mod wasm;
