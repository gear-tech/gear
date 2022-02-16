#![no_std]

use gstd::{debug, exec, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

fn make_fib(n: usize) -> Vec<i32> {
    let mut x = vec![1, 1];
    for i in 2..n {
        let next_x = x[i - 1] + x[i - 2];
        x.push(next_x)
    }
    x
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    MESSAGE_LOG.push(format!("New msg: {:?}", new_msg));
    debug!("fib gas_available: {}", exec::gas_available());

    if new_msg > 1000 {
        return;
    }

    msg::send(
        msg::source(),
        make_fib(new_msg as usize)[new_msg as usize - 1],
        exec::gas_available() - 1_000_000_000,
        0,
    );

    debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

    for log in MESSAGE_LOG.iter() {
        debug!(log);
    }
}
