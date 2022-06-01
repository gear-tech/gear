use gstd::{debug, exec, msg, prelude::*};

static mut RESULT: u8 = 0;

#[derive(Debug, Decode, TypeInfo)]
pub struct InputArgs {
    pub value: u8,
    pub times: u8,
}

gstd::metadata! {
    title: "demo sum over blocks",
    init:
    input: InputArgs,
}

#[gstd::async_init]
async fn init() {
    for num in nums {
        RESULT = RESULT.saturating_add(
            msg::send_and_wait_for_reply::<u8, u8>(exec::program_id(), num, 0)
                .expect("send message failed")
                .await
                .expect("get reply failed"),
        );
    }

    msg::reply(result, 0).unwrap();
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    msg::reply(RESULT, 0);
}
