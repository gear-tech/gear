#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::missing_safety_doc)]

extern crate alloc;

use alloc::vec;
use gstd::{msg, prelude::*};

const SHORT: usize = 100;
const LONG: usize = 10000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    let mut v = vec![0; SHORT];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
    msg::reply_bytes(&format!("init: {}", v.into_iter().sum::<usize>()), 0).unwrap();
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let mut v = vec![0; LONG];
    for (i, item) in v.iter_mut().enumerate() {
        *item = i * i;
    }
    msg::reply_bytes(&format!("handle: {}", v.into_iter().sum::<usize>()), 0).unwrap();
}

#[cfg(test)]
mod tests {
    use gtest::{Gas, Program, System};

    #[test]
    fn gas_burned() {
        let system = System::new();
        system.init_logger();

        let from = 42;

        let program = Program::current(&system);
        let res = program.send_bytes(from, "init");
        let init_gas_burned = res.main_gas_burned();
        println!("Init gas burned: {}", init_gas_burned);
        assert!(init_gas_burned > Gas::zero());

        let res = program.send_bytes(from, "handle");
        let handle_gas_burned = res.main_gas_burned();
        println!("Handle gas burned: {}", handle_gas_burned);
        assert!(handle_gas_burned > init_gas_burned);
    }
}
