use crate::{Call, Scheme};
use gstd::{collections::BTreeMap, msg, prelude::*, String, Vec};

pub(crate) static mut DATA: BTreeMap<String, Vec<u8>> = BTreeMap::new();
static mut SCHEME: Option<Scheme> = None;

fn process_fn<'a>(f: impl Fn(&'a Scheme) -> Option<&'a Vec<Call>>) {
    let scheme = unsafe { static_ref!(SCHEME).as_ref() }.expect("Should be set before access");
    let calls = f(scheme)
        .cloned()
        .unwrap_or_else(|| msg::load().expect("Failed to load payload"));

    let mut res = None;

    for call in calls {
        res = Some(call.process(res));
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let scheme = msg::load().expect("Failed to load payload");
    unsafe { SCHEME = Some(scheme) };

    process_fn(|scheme| Some(scheme.init()));
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    process_fn(Scheme::handle);
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    process_fn(Scheme::handle_reply);
}

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    process_fn(Scheme::handle_signal);
}
