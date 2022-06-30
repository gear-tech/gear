use gstd::{msg, prelude::*};
use shared::Package;

#[gstd::async_main]
async fn main() {
    let mut pkg: Package = msg::load().expect("invalid pow args");

    pkg.calc();

    msg::reply(pkg, 0).expect("send reply failed");
}
