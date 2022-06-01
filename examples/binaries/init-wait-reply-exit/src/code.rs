use gstd::{
    exec, msg,
    prelude::{vec, *},
};

#[gstd::async_init]
async fn init() {
    msg::send_and_wait_for_reply::<Vec<u8>, Vec<u8>>(msg::source(), vec![], 0)
        .expect("send message failed")
        .await
        .expect("get reply failed");
    exec::exit(msg::source());
}
