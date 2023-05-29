use crate::Call;
use gstd::{msg, BTreeMap, String, Vec};

pub(crate) static mut DATA: BTreeMap<String, Vec<u8>> = BTreeMap::new();

fn process(calls: Vec<Call>) {
    let mut res = None;

    for call in calls {
        res = Some(call.process(res));
    }
}

#[no_mangle]
extern "C" fn init() {
    let calls = msg::load().expect("Failed to load payload");

    process(calls)
}

#[no_mangle]
extern "C" fn handle() {
    let calls = msg::load().expect("Failed to load payload");

    process(calls)
}
