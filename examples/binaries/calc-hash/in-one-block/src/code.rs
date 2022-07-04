use gstd::{exec, msg, MessageId};
use shared::Package;

#[no_mangle]
unsafe extern "C" fn handle() {
    let mut pkg = msg::load::<Package>().expect("Invalid initial data");

    while !pkg.finished() {
        pkg = pkg.calc();
    }

    msg::reply(pkg.paths, 0).expect("send reply failed");
}
