#![no_std]

use gcore::{msg, H256};

fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    init_logger();
    // r#"
    // (module
    //   (import "env" "memory" (memory 1))
    //   (export "handle" (func $handle))
    //   (export "init" (func init))
    //   (func $handle)
    //   (func $init)
    // )"#
    let submitted_code: H256 = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
    let new_program_id = msg::create_program(submitted_code, b"default", b"", 10_000, 0);
    log::debug!("new program {:?}", new_program_id);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}