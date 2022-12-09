use crate::Package;
use gstd::msg;

#[no_mangle]
extern "C" fn handle() {
    let mut pkg = msg::load::<Package>().expect("Invalid initial data.");

    while !pkg.finished() {
        pkg.calc();
    }

    msg::reply(pkg.result(), 0).expect("Send reply failed.");
}
