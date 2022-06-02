use gstd::{exec, msg, Decode, Encode, ToString};
use pow::Package;

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[gstd::async_main]
async fn main() {
    let mut pkg: Package = msg::load().expect("invalid pow args");
    if pkg.exponent == pkg.ptr {
        msg::reply(pkg, 0).expect("send reply failed");
        return;
    }

    // the message handler
    gstd::debug!("\n\n--------> run once\n\n");
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

mod pow {
    use gstd::{Decode, Encode, TypeInfo};

    #[derive(Debug, Encode, Decode, TypeInfo)]
    pub struct Package {
        pub base: u8,
        pub exponent: u8,
        /// current exponent
        pub ptr: u8,
        /// the result of `pow(base, exponent)`
        pub result: u8,
    }

    impl Package {
        pub fn calc(mut self) -> Self {
            self.ptr += 1;
            self.result = self.base.saturating_mul(self.result);
            self
        }
    }
}
