use gstd::{
    exec, msg,
    prelude::{vec, *},
};

#[gstd::async_init]
async fn init() {
    msg::send_bytes_for_reply(msg::source(), vec![], 0)
        .expect("send message failed")
        .await
        .expect("ran into error-reply");
    exec::exit(msg::source());
}
