use gstd::msg;
use shared::Package;

#[gstd::async_main]
async fn main() {
    let mut pkg = msg::load::<Package>().expect("Invalid initial data");

    while !pkg.finished() {
        pkg = pkg.calc();
    }

    msg::reply(pkg.paths, 0).expect("send reply failed");
}
