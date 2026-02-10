# Signal codes testing

## Contents
1. [Summary](#summary)
1. [Testing technique](#testing-technique)
1. [Cases](#cases)
    1. [Execution signal codes](#execution)
        1. [Userspace panic](#userspace-panic)
        1. [Ran out of gas](#run-out-of-gas)
        1. [Backend error](#backend-error)
        1. [Memory overflow](#memory-overflow)
        1. [Unreachable instruction](#unreachable-instruction)
    1. [Non-execution signal codes](#non-execution)
        1. [Removed from waitlist](#removed-from-waitlist)

## Summary
<a name="summary"></a>

Our goal is to test _all_ cases where a signal code gets sent and ensure that it is sent and handled correctly.

Signal codes might be returned during the program's execution or when a significant event occurs outside the program's execution, such as when a message is removed from the waitlist.

You can find signal codes list in the `SignalCode` enum, located in [core-errors/src/simple.rs](src/simple.rs).

## Testing technique
<a name="testing-technique"></a>

In the following section you can find all cases of signal codes. Each case is accompanied by a reference program code and a corresponding reference test. This reference test will demonstrate the specific case, trigger the sending of signal code, and verify that the program receives the appropriate signal code.

Each test will reserve gas before action. This step ensures that the program doesn't run out of gas during the `handle_signal` execution.

Tests code will be written as if it were written in the `gear` pallet, because actual testing of these cases is done in the `gear` pallet.

## Cases
<a name="cases"></a>

### Execution signal codes (<small>`SignalCode::Execution`</small>)
<a name="execution"></a>

These signal codes are sent when the program's execution cannot proceed. Every one of these signal codes contains a `SimpleExecutionError` (refer to [core-errors/src/simple.rs](src/simple.rs)) within.

#### Userspace panic
<a name="userspace-panic"></a>

This signal code is sent when the runtime detects any Rust panic, such as through `panic!()`, `unreachable!()`, `assert!()`, `unimplemented!()`, and so on.

> In fact, this signal code is sent when the executor catches a Trap and issues a `TrapExplanation::Panic`. The `TrapExplanation::Panic` is sent every time the `gr_panic` syscall is invoked within the runtime. So to fully cover this case we need just to call the `gr_panic` function – and it will cover all other cases where `gr_panic` might be called.

<details>
<summary>Program to be uploaded</summary>

```rust
#![no_std]

use gstd::{
    ActorId,
    errors::{SignalCode, SimpleExecutionError},
    exec,
    msg,
    prelude::*
};

static mut INITIATOR: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { INITIATOR = msg::source() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    exec::system_reserve_gas(1_000_000_000).unwrap();

    panic!("Gotcha!");
}

#[unsafe(no_mangle)]
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

    let pid = get_last_program_id();

    run_to_next_block(None);

    // Ensure that program is uploaded and initialized correctly
    assert!(Gear::is_active(pid));
    assert!(Gear::is_initialized(pid));


    // Send the message to trigger signal sending
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(USER_1),
        pid,
        [].into(),
        GAS_LIMIT,
        0,
        false,
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
<a name="run-out-of-gas"></a>

This signal is sent when the trap `TrapExplanation::GasLimitExceeded` occurs. This trap may be caused by:
- Failing to charge gas during the program's execution because the gas runs out

    When this happens, the syscall `out_of_gas` gets called. This case can be tested simply by running empty loop in the program.

    <details>
    <summary>Program to be uploaded</summary>

    ```rust
    #![no_std]

    use gstd::{
        ActorId,
        errors::{SignalCode, SimpleExecutionError},
        exec,
        prelude::*,
        msg,
    };

    static mut INITIATOR: ActorId = ActorId::zero();

    #[unsafe(no_mangle)]
    extern "C" fn init() {
        unsafe { INITIATOR = msg::source() };
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle() {
        exec::system_reserve_gas(1_000_000_000).unwrap();

        #[allow(clippy::empty_loop)]
        loop {}
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle_signal() {
        let signal_received = msg::signal_code()
            .expect("Incorrect call")
            .expect("Unsupported code");

        if signal_received == SignalCode::Execution(SimpleExecutionError::RanOutOfGas) {
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
    fn test_signal_run_out_of_gas_works() {
        use demo_signal_run_out_of_gas::{WASM_BINARY};

        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0.encode(),
            GAS_LIMIT,
            0,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(Gear::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Send the message to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            [].into(),
            GAS_LIMIT,
            0,
            false,
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
- Gas runs out during plain or lazy pages memory access.

    This case can be tested by creating a program, that _only_ accesses memory, calculating gas for this program, and then running it with gass limit that is less than the calculated gas amount by a small margin. This will ensure that the program will run out of gas during memory access.

    <details>
    <summary>Program to be uploaded</summary>

    ```rust
    #![no_std]

    use gstd::{
        ActorId,
        errors::{SignalCode, SimpleExecutionError},
        exec,
        prelude::*,
        msg,
    };

    static mut INITIATOR: ActorId = ActorId::zero();

    #[unsafe(no_mangle)]
    extern "C" fn init() {
        unsafe { INITIATOR = msg::source() };
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle() {
        exec::system_reserve_gas(1_000_000_000).unwrap();

        const ARRAY_SIZE: usize = 1_000_000;
        let arr = [42u8; ARRAY_SIZE];

        for i in 0..ARRAY_SIZE {
            let value = arr[i];
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle_signal() {
        let signal_received = msg::signal_code()
            .expect("Incorrect call")
            .expect("Unsupported code");

        if signal_received == SignalCode::Execution(SimpleExecutionError::RanOutOfGas) {
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
    fn test_signal_run_out_of_gas_memory_access_works() {
        use demo_signal_run_out_of_gas_memory_access::{WASM_BINARY};

        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0.encode(),
            GAS_LIMIT,
            0,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(Gear::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Calculate gas for this action
        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            [].into(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        // Send the message to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            [].into(),
            min_limit - 1,
            0,
            false,
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

#### Backend error
<a name="backend-error"></a>

There are two cases of fails when this signal code is sent:
- `TrapExplanation::ForbiddenFunction`

    This case is sent when:
    - One of forbidden syscalls are called.

        In this case the syscall `gr_forbidden` will be called, resulting in execution stop. As of now, the only forbidden syscall are `gas_available` while calculating gas amount, so there is no way to test this case, because the message sent from the program won't be sent while calculating gas amount.


    - Some interactions with system actor are made:
        - Sending message
        - Sending message using reservation
        - Replying to message
        - Replying to message using reservation
        - Creating a new program with System ID as Program ID

        Below is the test for sending message to the system actor.
        <details>
        <summary>Program to be uploaded</summary>

        ```rust
        #![no_std]

        use gear_core::ids::ActorId;
        use gstd::{
            ActorId,
            errors::{SignalCode, SimpleExecutionError},
            exec,
            prelude::*,
            msg,
        };

        static mut INITIATOR: ActorId = ActorId::zero();

        #[unsafe(no_mangle)]
        extern "C" fn init() {
            unsafe { INITIATOR = msg::source() };
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle() {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            msg::send(ActorId::new(ActorId::SYSTEM.into()), "hello", 0)
                    .expect("cannot send message");
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle_signal() {
            let signal_received = msg::signal_code()
                .expect("Incorrect call")
                .expect("Unsupported code");

            if signal_received == SignalCode::Execution(SimpleExecutionError::BackendError) {
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
        fn test_signal_backend_error_system_actor_sending_works() {
            use demo_signal_backend_error_system_actor_sending::{WASM_BINARY};

            // Upload program
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                WASM_BINARY.to_vec(),
                DEFAULT_SALT.to_vec(),
                0.encode(),
                GAS_LIMIT,
                0,
            ));

            let pid = get_last_program_id();

            run_to_next_block(None);

            // Ensure that program is uploaded and initialized correctly
            assert!(Gear::is_active(pid));
            assert!(Gear::is_initialized(pid));

            // Send the message to trigger signal sending
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                [].into(),
                GAS_LIMIT,
                0,
                false,
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

- `TrapExplanation::UnrecoverableExt`

    This case is sent when:
    - A syscall `debug` gets called with invalid string.

        This test will not work in the `release` mode until the `gstd` crate is imported into the program code using the `debug` feature. Otherwise, the `gr_debug` syscall will be optimized out.

        <details>
        <summary>Program to be uploaded</summary>

        ```rust
        #![no_std]

        use gstd::{
            ActorId,
            debug,
            errors::{SignalCode, SimpleExecutionError},
            exec,
            prelude::*,
            msg,
        };

        static mut INITIATOR: ActorId = ActorId::zero();

        #[unsafe(no_mangle)]
        extern "C" fn init() {
            unsafe { INITIATOR = msg::source() };
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle() {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            #[allow(clippy::invalid_utf8_in_unchecked)]
            let invalid_string = unsafe { core::str::from_utf8_unchecked(&[0, 159, 146, 150]) };
            debug!("{}", invalid_string);
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle_signal() {
            let signal_received = msg::signal_code()
                .expect("Incorrect call")
                .expect("Unsupported code");

            if signal_received == SignalCode::Execution(SimpleExecutionError::BackendError) {
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
        fn test_signal_backend_error_incorrect_debug_string_works() {
            use demo_signal_backend_error_incorrect_debug_string::{WASM_BINARY};

            // Upload program
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                WASM_BINARY.to_vec(),
                DEFAULT_SALT.to_vec(),
                0.encode(),
                GAS_LIMIT,
                0,
            ));

            let pid = get_last_program_id();

            run_to_next_block(None);

            // Ensure that program is uploaded and initialized correctly
            assert!(Gear::is_active(pid));
            assert!(Gear::is_initialized(pid));

            // Send the message to trigger signal sending
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                [].into(),
                GAS_LIMIT,
                0,
                false,
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
    - Whenever `UnrecoverableExtError` happens, i.e. when `wait_up_to` called with 0 as parameter.

        <details>
        <summary>Program to be uploaded</summary>

        ```rust
        #![no_std]

        use gstd::{
            ActorId,
            errors::{SignalCode, SimpleExecutionError},
            exec,
            prelude::*,
            msg,
        };

        static mut INITIATOR: ActorId = ActorId::zero();

        #[unsafe(no_mangle)]
        extern "C" fn init() {
            unsafe { INITIATOR = msg::source() };
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle() {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            exec::wait_up_to(0);
        }

        #[unsafe(no_mangle)]
        extern "C" fn handle_signal() {
            let signal_received = msg::signal_code()
                .expect("Incorrect call")
                .expect("Unsupported code");

            if signal_received == SignalCode::Execution(SimpleExecutionError::BackendError) {
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
        fn test_signal_backend_error_unrecoverable_ext_works() {
            use demo_signal_backend_error_unrecoverable_ext::{WASM_BINARY};

            // Upload program
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                WASM_BINARY.to_vec(),
                DEFAULT_SALT.to_vec(),
                0.encode(),
                GAS_LIMIT,
                0,
            ));

            let pid = get_last_program_id();

            run_to_next_block(None);

            // Ensure that program is uploaded and initialized correctly
            assert!(Gear::is_active(pid));
            assert!(Gear::is_initialized(pid));

            // Send the message to trigger signal sending
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                [].into(),
                GAS_LIMIT,
                0,
                false,
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

    - When memory accessed out of bounds during lazy pages access.

        This case will be hard to test as there is no way to intentionally trigger lazy pages reading inside the program, so the test is not provided here.

#### Memory overflow
<a name="memory-overflow"></a>

This signal is sent when the `oom_panic` syscall gets called. This occurs when the program attempts to allocate more memory than it is allowed to, leading to `oom_panic` syscall and then to `TrapExplanation::ProgramAllocOutOfBounds` trap. To test this signal code, one can directly call the `oom_panic` syscall.

<details>
<summary>Program to be uploaded</summary>

```rust
#![no_std]

use gstd::{
    ActorId,
    errors::{SignalCode, SimpleExecutionError},
    exec,
    ext::oom_panic,
    prelude::*,
    msg,
};

static mut INITIATOR: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { INITIATOR = msg::source() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    exec::system_reserve_gas(1_000_000_000).unwrap();

    oom_panic();
}

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    let signal_received = msg::signal_code()
        .expect("Incorrect call")
        .expect("Unsupported code");

    if signal_received == SignalCode::Execution(SimpleExecutionError::MemoryOverflow) {
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
fn test_signal_memory_overflow_works() {
    use demo_signal_memory_overflow::{WASM_BINARY};

    // Upload program
    assert_ok!(Gear::upload_program(
        RuntimeOrigin::signed(USER_1),
        WASM_BINARY.to_vec(),
        DEFAULT_SALT.to_vec(),
        0.encode(),
        GAS_LIMIT,
        0,
    ));

    let pid = get_last_program_id();

    run_to_next_block(None);

    // Ensure that program is uploaded and initialized correctly
    assert!(Gear::is_active(pid));
    assert!(Gear::is_initialized(pid));


    // Send the message to trigger signal sending
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(USER_1),
        pid,
        [].into(),
        GAS_LIMIT,
        0,
        false,
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

#### Unreachable instruction
<a name="unreachable-instruction"></a>

This signal is sent when the `TrapExplanation::Unknown` trap is triggered. This can occur when:
- There's an attempt to free memory that hasn't been allocated.

    This error gets explicitly returned when the `free` syscall gets called with number of memory page that was not allocated. To test this case, one can simply call the `free` syscall using an invalid page number.

    Since `free` syscall is not explicitly exported in any of user-space libraries, the `extern "C"` function import must be used to call it.

    For this test, `usize::MAX` is used as invalid page number. In our memory system, `0` is considered a valid page number, whereas the `usize::MAX` page number is reserved.

    <details>
    <summary>Program to be uploaded</summary>

    ```rust
    #![no_std]

    use gstd::{
        ActorId,
        errors::{SignalCode, SimpleExecutionError},
        exec,
        prelude::*,
        msg,
    };

    static mut INITIATOR: ActorId = ActorId::zero();

    extern "C" {
        fn free(ptr: *mut u8) -> *mut u8;
    }

    #[unsafe(no_mangle)]
    extern "C" fn init() {
        unsafe { INITIATOR = msg::source() };
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle() {
        exec::system_reserve_gas(1_000_000_000).unwrap();

        unsafe {
            free(usize::MAX as *mut u8);
        }
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle_signal() {
        let signal_received = msg::signal_code()
            .expect("Incorrect call")
            .expect("Unsupported code");

        if signal_received == SignalCode::Execution(SimpleExecutionError::UnreachableInstruction) {
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
    fn test_signal_unreachable_instruction_incorrect_free_works() {
        use demo_signal_unreachable_instruction_incorrect_free::{WASM_BINARY};

        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0.encode(),
            GAS_LIMIT,
            0,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(Gear::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Send the message to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            [].into(),
            GAS_LIMIT,
            0,
            false,
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


- Terminating backend with `failure` reason and failed executor.

    To test this case, a `success` termination reason must be received during a backend failure. This case cannot be triggered by the program itself, so the test is not provided here.
- Called plain `unreachable` WASM instruction.

    > Please note that `unreachable!()` Rust macro is not the same as `unreachable` WASM instruction. The Rust macro simply calls `panic!()` and will not cause this signal code to be sent.

    <details>
    <summary>Program to be uploaded</summary>

    ```rust
    #![no_std]

    use gstd::{
        ActorId,
        errors::{SignalCode, SimpleExecutionError},
        exec,
        prelude::*,
        msg,
    };

    static mut INITIATOR: ActorId = ActorId::zero();

    #[unsafe(no_mangle)]
    extern "C" fn init() {
        unsafe { INITIATOR = msg::source() };
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle() {
        exec::system_reserve_gas(1_000_000_000).unwrap();

        #[cfg(target_arch = "wasm32")]
        core::arch::wasm32::unreachable();
    }

    #[unsafe(no_mangle)]
    extern "C" fn handle_signal() {
        let signal_received = msg::signal_code()
            .expect("Incorrect call")
            .expect("Unsupported code");

        if signal_received == SignalCode::Execution(SimpleExecutionError::UnreachableInstruction) {
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
    fn test_signal_unreachable_instruction_wasm_works() {
        use demo_signal_unreachable_instruction_wasm::{WASM_BINARY};

        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0.encode(),
            GAS_LIMIT,
            0,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(Gear::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Send the message to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            [].into(),
            GAS_LIMIT,
            0,
            false,
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

### Non-execution signal codes
<a name="non-execution"></a>

#### Removed from waitlist (<small>`SignalCode::RemovedFromWaitlist`</small>)
<a name="removed-from-waitlist"></a>

This signal will be sent whenever the `remove_from_waitlist` method of `TaskHandler` is called (refer to the `gear` pallet, `manager.rs`). To test this, one can trigger wait in program. Once the waiting period expires, `remove_from_waitlist` will be invoked and will result in the sending of the signal.


<details>
<summary>Program to be uploaded</summary>

```rust
#![no_std]

use gstd::{
    ActorId,
    errors::{SignalCode, SimpleExecutionError},
    exec,
    prelude::*,
    msg,
};

static mut INITIATOR: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { INITIATOR = msg::source() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    exec::system_reserve_gas(1_000_000_000).unwrap();

    exec::wait();
}

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    let signal_received = msg::signal_code()
        .expect("Incorrect call")
        .expect("Unsupported code");

    if signal_received == SignalCode::RemovedFromWaitlist {
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
fn test_signal_removed_from_waitlist() {
    use demo_signal_removed_from_waitlist::{WASM_BINARY};

    // Upload program
    assert_ok!(Gear::upload_program(
        RuntimeOrigin::signed(USER_1),
        WASM_BINARY.to_vec(),
        DEFAULT_SALT.to_vec(),
        0.encode(),
        GAS_LIMIT,
        0,
    ));

    let pid = get_last_program_id();

    run_to_next_block(None);

    // Ensure that program is uploaded and initialized correctly
    assert!(Gear::is_active(pid));
    assert!(Gear::is_initialized(pid));

    // Send the message to trigger signal sending
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(USER_1),
        pid,
        [].into(),
        GAS_LIMIT,
        0,
        false,
    ));

    run_to_next_block(None);

    let mid = get_last_message_id();

    // Ensuring that gas is reserved
    assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

    // Getting block number when waitlist expiration should happen
    let mut expiration = None;

    System::events().iter().for_each(|e| {
        if let MockRuntimeEvent::Gear(Event::MessageWaited {
            expiration: exp, ..
        }) = e.event
        {
            expiration = Some(exp);
        }
    });

    let expiration = expiration.unwrap();

    // Hack to fast spend blocks till expiration
    System::set_block_number(expiration - 1);
    Gear::set_block_number(expiration - 1);

    // Expiring that message
    run_to_next_block(None);

    // Ensure that signal code sent is signal code we saved
    let mail_msg = get_last_mail(USER_1);
    assert_eq!(mail_msg.payload_bytes(), true.encode());
}
```
</details>
