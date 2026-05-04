fn main() {}

#[gstd::async_init(handle_signal = custom_handle_signal)]
async fn init() {}

fn custom_handle_signal() {}
