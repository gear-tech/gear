fn main() {}

#[gstd::async_main(handle_signal = custom_handle_signal)]
async fn main() {}

fn custom_handle_signal() {}
