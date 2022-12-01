#![no_main]

#[gstd::async_init(handle_signal = custom_handle_reply)]
async fn init() {}

#[gstd::async_main(handle_signal = custom_handle_reply)]
async fn main() {}

fn custom_handle_reply() {}
