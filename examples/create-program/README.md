# `create_program` syscall example
In order to define "factory" logic in your program
you should do the following steps:
1. Invoke `submit_code` extrinsic call to set your "child" program code in storage. A "child" code is the one which you will instantiate from the other ("parent") program.
2. Get the code hash wrapped in `CodeSaved` event generated after successful extrinsic call.
3. Use the code hash as a corresponding parameter in `create_program_with_gas` sys-call.

In this example we deploy the next code (in wat format):
```
let wat = r#"(module
  (import "env" "memory" (memory 1))
  (export "handle" (func $handle))
  (export "init" (func init))
  (func $handle)
  (func $init)
)"#;

let code_bytes = wabt::Wat2Wasm::new()
    .validate(false)
    .convert(wat)
    .expect("failed to parse module")
    .as_ref()
    .to_vec();
// [0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 2, 15, 1, 3, 101, 110, 118, 6, 109, 101, 109, 111, 114, 121, 2, 0, 1, 3, 3, 2, 0, 0, 7, 17, 2, 6, 104, 97, 110, 100, 108, 101, 0, 0, 4, 105, 110, 105, 116, 0, 1, 10, 7, 2, 2, 0, 11, 2, 0, 11]

let code_hash: sp_core::H256 = sp_io::hashing::blake2_256(&code_bytes).into();
// 0xabf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a
```
