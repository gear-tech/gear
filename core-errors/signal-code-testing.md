# Signal codes testing

## Contents
1. Summary
1. Cases
    1. Panic (execution errors) signal codes
        1. Userspace panic
        1. Ran out of gas
        1. Backend error
        1. Memory overflow
        1. Unreachable instruction
    1. Non-execution signal codes
        1. Removed from waitlist

## Summary
We are testing the signal codes that are returned by the runtime. The signal codes may be returned by the runtime during the execution of program or when something meaningful happens outside of the execution of the program (like the message gets removed from the waitlist).

We want to test all the cases where a signals code gets sent and make sure that the program acquired the correct signal code, so that the program can react accordingly. 

The signal codes are defined in the `SignalCode` enum in [core-errors/src/simple.rs](./src/simple.rs).

Below we will list all the cases that we want to test and a reference test for each case. The reference test is a test that demonstrates the case and checks that the program got the correct signal code.

## Cases

### Panic (execution errors) signal codes

These signal codes are sent when the execution of the program cannot be continued. All of these signal codes contains a `SimpleExecutionError` (see [core-errors/src/simple.rs](./src/simple.rs)) inside.

#### Userspace panic

This signal code is sent whenever runtime notices every Rust panic, including the ones like `unreachable!()`, `assert!()`, `unimplemented!()`, etc.

> In fact, this signal code is sent whenever the executor catches a Trap and sends a `TrapExplanation::Panic`. The `TrapExplanation::Panic` is sent every time the `gr_panic` function is called inside the runtime. So to fully cover this case we need just to somehow call the `gr_panic` function – and it will cover all other cases where `gr_panic` may be called.

<details>
<summary>Program to be uploaded</summary>

```rust
#![no_std]

use gstd::{errors::{SignalCode, SimpleExecutionError}, msg, prelude::*};

static mut INITIATOR: ActorId = ActorId::zero();

#[no_mangle]
extern "C" fn init() {
    unsafe { INITIATOR = msg::source() };
}

#[no_mangle]
extern "C" fn handle() {
    panic!("Gotcha!");
}

#[no_mangle]
extern "C" fn handle_signal() {
    let signal_received = msg::signal_code()
        .expect("Incorrect call")
        .expect("Unsupported code");

    if signal_received == SignalCode::Execution(SimpleExecutionError::UserspacePanic) {
        msg::send(unsafe { INITIATOR }, true, 0).unwrap();
    } else {
        msg::send(unsafe { INITIATOR }, false, 0).unwrap();
    }
}
```

</details>

<details>
<summary>Test</summary>

```rust
const USER_1: AccountId = 1;
const DEFAULT_SALT: &[u8; 4] = b"salt";
const GAS_LIMIT: u64 = 10_000_000_000;

#[test]
fn test_userspace_panic_works() {
    use demo_signal_panic::{WASM_BINARY};

    // Upload program
    assert_ok!(Gear::upload_program(
        RuntimeOrigin::signed(USER_1),
        WASM_BINARY.to_vec(),
        DEFAULT_SALT.to_vec(),
        0.encode(),
        GAS_LIMIT,
        0,
    ));

    // Ensure that program is uploaded correctly
    let pid = get_last_program_id();
    assert!(Gear::is_active(pid));

    // Initialize program
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(USER_1),
        pid,
        DEFAULT_SALT.to_vec(),
        GAS_LIMIT,
        0,
    ));

    run_to_next_block(None);

    // Ensure that program is initialized correctly
    assert!(Gear::is_initialized(pid));

    // Send the message to trigger signal sending
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(USER_1),
        pid,
        b"please, panic".encode(),
        GAS_LIMIT,
        0,
    ));

    run_to_next_block(None);

    let mid = get_last_message_id();

    // Assert that system reserve gas node is removed
    assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));
    run_to_next_block(None);
    assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

    // Ensure that signal code sent is signal code we saved
    let mail_msg = get_last_mail(USER_1);
    assert_eq!(mail_msg.payload_bytes(), true.encode());
}
```
</details>

#### Ran out of gas

#### Backend error

#### Memory overflow

#### Unreachable instruction

### Non-execution signal codes

#### Removed from waitlist