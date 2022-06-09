use crate::pow::Package;
use gstd::{exec, msg, Decode, Encode, ToString};

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[gstd::async_main]
async fn main() {
    let mut pkg: Package = msg::load().expect("invalid pow args");
    if pkg.exponent == pkg.ptr {
        msg::reply(pkg, 0).expect("send reply failed");
        return;
    }

    // start calculating
    pkg = msg::send_and_wait_for_reply::<Package, Package>(
        exec::program_id(),
        pkg.calc(),
        exec::gas_available().into(),
    )
    .expect("send message failed")
    .await
    .expect("get reply failed")
    .into();

    msg::reply(pkg, 0).expect("send reply failed");
}
