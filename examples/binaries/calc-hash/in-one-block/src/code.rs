use gstd::msg;
use shared::Package;

#[no_mangle]
unsafe extern "C" fn handle() {
    let mut pkg = msg::load::<Package>().expect("Invalid initial data.");

    while !pkg.finished() {
        pkg.calc();
    }

    msg::reply(pkg.result, 0).expect("Send reply failed.");
}
