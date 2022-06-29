use crate::{GasMeter, Package};
use core::default::Default;
use gstd::{exec, msg, Encode};

static mut GAS_METER: GasMeter = GasMeter {
    base: 0,
    gas_spent: 0,
    ptr: 0,
};

#[gstd::async_init]
async unsafe fn init() {
    GAS_METER = GasMeter::new(exec::gas_available());
}

#[gstd::async_main]
async fn main() {
    let mut pkg: Package = msg::load().expect("invalid pow args");

    unsafe {
        GAS_METER.load(pkg.base);

        loop {
            if !GAS_METER.spin(exec::gas_available()) {
                break;
            }

            gstd::debug!("hello");
            pkg.calc();

            if pkg.finished() {
                msg::reply((true, pkg.result, pkg).encode(), 0).expect("send reply failed");
                return;
            }
        }
    }

    msg::reply((false, pkg.result, pkg).encode(), 0).expect("send reply failed");
}
