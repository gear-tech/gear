#![cfg_attr(not(feature = "std"), no_std)]
use parity_scale_codec::{Decode, Encode};

#[cfg(not(feature = "std"))]
mod wasm;

#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq)]
pub enum Kind {
    Send,
    SendWithGas(u64),
    SendBytes,
    SendBytesWithGas(u64),
    SendCommit,
    SendCommitWithGas(u64),
}
