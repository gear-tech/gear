use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
extern "C" fn handle() {
    let size = msg::load::<i32>().expect("Failed to load `i32`") as usize;

    let request = format!("Request: size = {size}");

    debug!(request);
    unsafe { MESSAGE_LOG.push(request) };

    let vec = vec![42u8; size];
    let last_idx = size - 1;

    debug!("vec.len() = {:?}", vec.len());
    debug!(
        "vec[{}]: {:p} -> {:#04x}",
        last_idx, &vec[last_idx], vec[last_idx]
    );

    msg::reply(size as i32, 0).expect("Failed to send reply");

    // The test idea is to allocate two wasm pages and check this allocation,
    // so we must skip `v` destruction.
    core::mem::forget(vec);

    debug!("Total requests amount: {}", unsafe { MESSAGE_LOG.len() });
    unsafe {
        MESSAGE_LOG.iter().for_each(|log| debug!(log));
    }
}
