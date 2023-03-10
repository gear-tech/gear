#![no_std]

use gstd::{debug, msg, prelude::*};

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
extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    unsafe { MESSAGE_LOG.push(format!("New msg: {new_msg:?}")) };

    if new_msg > 1000 {
        return;
    }

    msg::send(
        msg::source(),
        make_fib(new_msg as usize)[new_msg as usize - 1],
        0,
    )
    .unwrap();

    debug!("{:?} total message(s) stored: ", unsafe {
        MESSAGE_LOG.len()
    });

    for log in unsafe { MESSAGE_LOG.iter() } {
        debug!(log);
    }
}
