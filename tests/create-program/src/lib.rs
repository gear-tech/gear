#![no_std]

use gstd::{debug, msg, prog, CodeHash, String};

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
pub unsafe extern "C" fn handle() {
    let command = String::from_utf8(msg::load_bytes()).expect("Unable to decode string");
    let submitted_code: CodeHash =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
            .into();

    match command.as_ref() {
        "default" => {
            // Assume that the code of the deploying program was submitted by `submit_code`
            // extrinsic and we got its hash. For more details please read README file.
            let new_program_id = prog::create_program_with_gas(
                submitted_code,
                COUNTER.to_le_bytes(),
                b"unique",
                10_000,
                0,
            );
            debug!("A new program is created {:?}", new_program_id);

            let msg_id = msg::send(new_program_id, b"", 0);
            debug!("Sent to a new program message with id {:?}", msg_id);

            COUNTER += 1;
        }
        "duplicate" => {
            let new_program_id = prog::create_program_with_gas(
                submitted_code,
                (COUNTER - 1).to_le_bytes(),
                b"not_unique",
                10_000,
                0,
            );
            debug!("A new program is created {:?}", new_program_id);

            let msg_id = msg::send(new_program_id, b"", 0);
            debug!("Sent to a new program message with id {:?}", msg_id);
        }
        _ => {
            panic!("Unknown option");
        }
    }
}

#[cfg(test)]
mod tests {
    use gtest::{System, Program};

    #[test]
    fn test_simple() {
        let sys = System::new();

        // Store child
        let code_hash_stored = sys.submit_code("./child_contract.wasm");
        let new_actor_id_expected = Program::calculate_program_id(code_hash_stored, &0i32.to_le_bytes());

        // Create program
        let program = Program::current_with_id(&sys, 100);
        // init function
        let res = program.send_bytes(10001, "EMPTY");
        assert!(!res.main_failed());
        assert_eq!(res.initialized_programs().len(), 1);
        // handle function
        let res = program.send_bytes(10001, "default");
        assert!(!res.main_failed());
        assert!(!res.others_failed());
        assert_eq!(res.initialized_programs().len(), 2);

        let (new_actor_id_actual, new_actor_code_hash) = res.initialized_programs().last().copied().unwrap();
        assert_eq!(new_actor_id_expected, new_actor_id_actual);
        assert_eq!(Some(code_hash_stored), new_actor_code_hash);

        let program = sys.get_program(new_actor_id_expected);

        let res = program.send_bytes(10001, "default");
        assert!(!res.main_failed());
    }
}