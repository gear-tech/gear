use gstd::{msg, prelude::*};

const SHORT: usize = 100;
const LONG: usize = 10000;

#[no_mangle]
extern "C" fn init() {
    let mut v = vec![0; SHORT];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
    msg::reply_bytes(format!("init: {}", v.into_iter().sum::<usize>()), 0).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    let mut v = vec![0; LONG];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
}
