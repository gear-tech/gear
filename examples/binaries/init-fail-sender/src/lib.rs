#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub fn system_reserve() -> u64 {
    gstd::Config::system_reserve()
}

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{exec, msg, ActorId};

    #[gstd::async_init]
    async fn init() {
        let value_receiver: ActorId = msg::load().unwrap();

        msg::send_bytes_with_gas(value_receiver, [], 10_000, 1_000).unwrap();
        msg::reply_bytes_with_gas_for_reply([], 10_000, 0)
            .unwrap()
            .await;
        panic!();
    }
}
