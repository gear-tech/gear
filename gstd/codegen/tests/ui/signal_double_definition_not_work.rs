#![no_main]

#[gstd::async_init(handle_signal = custom_handle_signal)]
async fn init() {}

#[gstd::async_main(handle_signal = custom_handle_signal)]
async fn main() {}

fn custom_handle_signal() {}
