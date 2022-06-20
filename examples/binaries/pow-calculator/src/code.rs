use crate::{GasMeter, Package};
use gstd::{exec, msg, Decode, Encode, ToString};

#[gstd::async_main]
async unsafe fn main() {
    let mut pkg: Package = msg::load().expect("invalid pow args");
    let mut gas_meter = GasMeter::new(exec::gas_available(), pkg.base);

    loop {
        if !gas_meter.spin(exec::gas_available()) {
            break;
        }

        pkg.calc();

        if pkg.finished() {
            msg::reply((true, pkg.result, pkg).encode(), 0).expect("send reply failed");
            return;
        }
    }

    msg::reply((false, pkg.result, pkg).encode(), 0).expect("send reply failed");
}
