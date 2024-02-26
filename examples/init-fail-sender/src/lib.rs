#![cfg_attr(not(feature = "std"), no_std)]

pub fn system_reserve() -> u64 {
    gstd::Config::system_reserve()
}

pub fn reply_duration() -> u32 {
    1
}

#[cfg(not(feature = "std"))]
mod wasm;
