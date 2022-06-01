use gstd::{exec, msg, Decode, ToString};
use pow::{Package, Pow};

gstd::metadata! {
    title: "demo pow",
    init:
        input: Pow,
}

#[gstd::async_init]
async fn init() {
    gstd::debug!("initing...");
    let mut pkg: Package = msg::load::<Pow>().expect("invalid pow args").into();

    loop {
        if pkg.done {
            break;
        }

        pkg = msg::send_and_wait_for_reply::<Package, Package>(
            exec::program_id(),
            pkg.calc(),
            exec::gas_available().into(),
        )
        .expect("send message failed")
        .await
        .expect("get reply failed");
    }

    msg::reply(pkg.result, 0).expect("reply failed");
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    gstd::debug!("handling...");
    msg::reply(
        msg::load::<Package>()
            .expect("decode package failed")
            .calc(),
        exec::gas_available().into(),
    );
}

mod pow {
    use gstd::{Decode, Encode, TypeInfo};

    #[derive(Debug, Encode, Decode, TypeInfo)]
    pub struct Pow {
        pub base: u8,
        pub exponent: u8,
    }

    #[derive(Debug, Encode, Decode, TypeInfo)]
    pub struct Package {
        pub done: bool,
        pub pow: Pow,
        /// current exponent
        pub ptr: u8,
        /// the result of `pow(base, exponent)`
        pub result: u8,
    }

    impl Package {
        pub fn calc(mut self) -> Self {
            if self.done == true {
                return self;
            } else if self.ptr == self.pow.exponent {
                self.done = true;
                return self;
            }

            self.ptr += 1;
            self.result = self.pow.base.saturating_mul(self.result);
            self
        }
    }

    impl From<Pow> for Package {
        fn from(pow: Pow) -> Self {
            Self {
                pow,
                ptr: 0,
                result: 1,
                done: false,
            }
        }
    }
}
