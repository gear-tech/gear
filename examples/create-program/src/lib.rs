#![no_std]

use gstd::{debug, msg, prog, CodeId, String};

static mut COUNTER: i32 = 0;

/// Creates the following program:
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
extern "C" fn handle() {
    let command = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Unable to decode string");
    let submitted_code: CodeId =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
            .into();

    match command.as_ref() {
        "default" => {
            // Assume that the code of the deploying program was submitted by `submit_code`
            // extrinsic and we got its hash. For more details please read README file.
            let (_message_id, new_program_id) = prog::create_program_with_gas(
                submitted_code,
                unsafe { COUNTER.to_le_bytes() },
                b"unique",
                10_000_000_000,
                0,
            )
            .unwrap();
            debug!("A new program is created {:?}", new_program_id);

            let msg_id = msg::send(new_program_id, b"", 0).unwrap();
            debug!("Sent to a new program message with id {:?}", msg_id);

            unsafe { COUNTER += 1 };
        }
        "duplicate" => {
            let (_message_id, new_program_id) = prog::create_program_with_gas(
                submitted_code,
                unsafe { (COUNTER - 1).to_le_bytes() },
                b"not_unique",
                10_000_000_000,
                0,
            )
            .unwrap();
            debug!("A new program is created {:?}", new_program_id);

            let msg_id = msg::send(new_program_id, b"", 0).unwrap();
            debug!("Sent to a new program message with id {:?}", msg_id);
        }
        _ => {
            panic!("Unknown option");
        }
    }
}
