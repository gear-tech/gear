#![no_std]

use gcore::{msg, H256};
use gstd::debug;

/// Creates the next program
/// ```
/// let default_program = r#"
/// (module
///   (import "env" "memory" (memory 1))
///   (export "handle" (func $handle))
///   (export "init" (func init))
///   (func $handle)
///   (func $init)
/// )"#;
/// ```
#[no_mangle]
pub unsafe extern "C" fn handle() {
    // Assume that deploying program code was submitted by `submit_code` extrinsic and we got its hash.
    // For more info please refer to [guide](todo [sab]).
    let submitted_code: H256 = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
    let new_program_id = msg::create_program(submitted_code, b"default", b"", 10_000, 0);
    debug!("A new program is created {:?}", new_program_id);

    let msg_id = msg::send(new_program_id, b"", 10_000, 0);
    debug!("Sent to a new program message with id {:?}", msg_id);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}