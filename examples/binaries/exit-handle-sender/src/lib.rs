#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use gstd::{ActorId, Vec};

#[derive(Debug, Decode, Encode)]
pub enum Input {
    SendMessage {
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    Exit(ActorId),
}

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::*;
    use gstd::{exec, msg};

    #[no_mangle]
    extern "C" fn handle() {
        let input: Input = msg::load().unwrap();

        match input {
            Input::SendMessage {
                destination,
                payload,
                value,
            } => {
                msg::send_bytes(destination, payload, value).unwrap();
            }
            Input::Exit(destination) => {
                exec::exit(destination);
            }
        }
    }
}
