//! Testing integration level of sys-calss
//!
//! Integration level is the level between the user (`gcore`/`gstd`) and `core-backend`.
//! Tests here does not check complex business logic, but only the fact that all the
//! requested data is received properly, i.e., pointers receive expected types.
//!
//! `gr_read`is tested in the `test_sys_calls` program by calling `msg::load` to decode each sys-call type.
//! `gr_exit` and `gr_wait*` call are not intended to be tested with the integration level tests, but only
//! with business logic tests in the separate module.

use super::*;
use crate::mock::Timestamp;
use gear_backend_common::SysCallNames;
use gear_core::ids::ReservationId;
use primitive_types::H256;
use test_sys_calls::{Kind, WASM_BINARY as SYS_CALLS_TESTER_WASM_BINARY};

#[test]
fn test_sys_calls_integrity() {
    use SysCallNames::*;

    SysCallNames::all().for_each(|sys_call| {
        match sys_call {
            Send => check_send(0),
            SendWGas => check_send(25_000_000_000),
            SendCommit => check_send_raw(0),
            SendCommitWGas => check_send_raw(25_000_000_000),
            SendInit | SendPush => {/* skipped, due to test being run in SendCommit* variants */},
            Reply => check_reply(0),
            ReplyWGas => check_reply(25_000_000_000),
            ReplyCommit => check_reply_raw(0),
            ReplyCommitWGas => check_reply_raw(25_000_000_000),
            ReplyTo => check_gr_reply_details(),
            ReplyPush => {/* skipped, due to test being run in SendCommit* variants */},
            CreateProgram => check_create_program(0),
            CreateProgramWGas => check_create_program(25_000_000_000),
            Read => {/* checked in all the calls internally */},
            Size => check_gr_size(),
            ExitCode => {/* checked in reply_to */},
            MessageId => check_gr_message_id(),
            ProgramId => check_gr_program_id(),
            Source => check_gr_source(),
            Value => check_gr_value(),
            BlockHeight => check_gr_block_height(),
            BlockTimestamp => check_gr_block_timestamp(),
            Origin => check_gr_origin(),
            GasAvailable => check_gr_gas_available(),
            ValueAvailable => check_gr_value_available(),
            Exit | Leave | Wait | WaitFor | WaitUpTo | Wake | Debug => {/* test here aren't required, read module docs for more info */},
            Alloc => check_mem(false),
            Free => check_mem(true),
            OutOfGas | OutOfAllowance => { /* TODO [SAB] */}
            Error => check_gr_err(),
            Random => check_gr_random(),
            ReserveGas => check_gr_reserve_gas(),
            UnreserveGas => check_gr_unreserve_gas(),
        }
    })
}


// Checks `alloc` by default and `free` by param
fn check_mem(check_free: bool) {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "alloc" (func $alloc (param i32) (result i32)))
        (import "env" "free" (func $free (param i32)))
        (export "init" (func $init))
        (export "handle" (func $handle))
        (func $init
            ;; allocate 2 more pages with expected starting index 1
            (block
                i32.const 0x2
                call $alloc
                i32.const 0x1
                i32.eq
                br_if 0
                unreachable
            )
            ;; put to page with index 2 (the third) some value
            (block
                i32.const 0x20001
                i32.const 0x63
                i32.store
            )
            ;; put to page with index 1 (the second) some value
            (block
                i32.const 0x10001
                i32.const 0x64
                i32.store
            )
            ;; check it has the value
            (block
                i32.const 0x10001
                i32.load
                i32.const 0x64
                i32.eq
                br_if 0
                unreachable
            )
            ;; remove page with index 1 (the second page)
            (block
                i32.const 0x1
                call $free
            )
        )
        (func $handle
            ;; check that the second page is empty
            (block
                i32.const 0x10001
                i32.load
                i32.const 0x0
                i32.eq
                br_if 0
                unreachable
            )
            ;; check that the third page has data
            (block
                i32.const 0x20001
                i32.load
                i32.const 0x63
                i32.eq
                br_if 0
                unreachable
            )
        )
    )"#;
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat);
        assert_ok!(Gear::upload_program(RuntimeOrigin::signed(USER_1), code.to_bytes(), DEFAULT_SALT.to_vec(), EMPTY_PAYLOAD.to_vec(), 50_000_000_000, 0));

        let pid = get_last_program_id();
        run_to_next_block(None);

        if free {
            assert_ok!(Gear::send_message(RuntimeOrigin::signed(USER_1), pid, EMPTY_PAYLOAD.to_vec(), 50_000_000_000, 0));
            run_to_next_block(None);
        }
    })
}

// Depending on `gas` param will be `gr_create_program` or `gr_create_program_wgas.
fn check_create_program(gas: u64) {
    run_tester(|_, _| {
        let next_user_mid = get_next_message_id(USER_1);
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0).into_origin();
        let salt = 10u64;
        let expected_pid =
            generate_program_id(&ProgramCodeKind::Default.to_bytes(), &salt.to_le_bytes())
                .into_origin();

        let mp = Kind::CreateProgram(salt, gas, (expected_mid.into(), expected_pid.into()))
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_err() {
    run_tester(|_, _| {
        let message_value = u128::MAX;
        let expected_err = ExtError::Message(MessageError::NotEnoughValue {
            message_value,
            value_left: 0,
        });

        let mp = Kind::Error(message_value, expected_err).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

// Depending on `gas` param will be `gr_send` or `gr_send_wgas`.
fn check_send(gas: u64) {
    run_tester(|_, _| {
        let next_user_mid = get_next_message_id(USER_1);
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0).into_origin();

        let mp = Kind::Send(gas, expected_mid.into()).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

/// Tests send_init, send_push, send_commit or send_commit_wgas depending on `gas` param.
fn check_send_raw(gas: u64) {
    run_tester(|_, _| {
        let payload = b"HI!!";
        let next_user_mid = get_next_message_id(USER_1);
        // Program increases local nonce by sending messages twice before `send_init`.
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 2).into_origin();

        let post_test = move || {
            assert!(
                MailboxOf::<Test>::iter_key(USER_1)
                    .any(|(m, _)| m.id() == MessageId::from_origin(expected_mid)
                        && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::SendRaw(payload.to_vec(), gas, expected_mid.into())
            .encode()
            .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

fn check_gr_size() {
    run_tester(|_, _| {
        // One byte for enum variant, four bytes for u32 value
        let expected_size = 5;

        let mp = Kind::Size(expected_size).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_message_id() {
    run_tester(|_, _| {
        let next_user_mid = get_next_message_id(USER_1);

        let mp = Kind::MessageId(next_user_mid.into_origin().into())
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_program_id() {
    run_tester(|id, _| {
        let mp = Kind::ProgramId(id.into_origin().into()).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_source() {
    run_tester(|_, _| {
        let mp = MessageParamsBuilder::new(Kind::Source(USER_2.into_origin().into()).encode())
            .with_sender(USER_2);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_value() {
    run_tester(|_, _| {
        let sending_value = u16::MAX as u128;
        let mp = MessageParamsBuilder::new(Kind::Value(sending_value).encode())
            .with_value(sending_value);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_value_available() {
    run_tester(|_, _| {
        let sending_value = 10_000;
        // Program sends 2000
        let mp = MessageParamsBuilder::new(Kind::ValueAvailable(sending_value - 2000).encode())
            .with_value(sending_value);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// Depending on `gas` param will be `gr_reply` or `gr_reply_wgas`.
fn check_reply(gas: u64) {
    run_tester(|_, _| {
        let next_user_mid = get_next_message_id(USER_1);
        let expected_mid = MessageId::generate_reply(next_user_mid, 0).into_origin();

        let mp = Kind::Reply(gas, expected_mid.into()).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// Tests `reply_push` and `reply_commit` or `reply_commit_wgas` depending on `gas` value.
fn check_reply_raw(gas: u64) {
    run_tester(|_, _| {
        let payload = b"HI!!";
        let next_user_mid = get_next_message_id(USER_1);
        // Program increases local nonce by sending messages twice before `send_init`.
        let expected_mid = MessageId::generate_reply(next_user_mid, 0).into_origin();

        let post_test = move || {
            assert!(
                MailboxOf::<Test>::iter_key(USER_1)
                    .any(|(m, _)| m.id() == MessageId::from_origin(expected_mid)
                        && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::ReplyRaw(payload.to_vec(), gas, expected_mid.into())
            .encode()
            .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

// Tests `reply_to` and  `exit_code`
fn check_gr_reply_details() {
    run_tester(|tester_pid, _| {
        let next_user_mid = get_next_message_id(USER_1);
        // Program increases local nonce by sending messages twice before `send_init`.
        let expected_mid = MessageId::generate_reply(next_user_mid, 0);

        // trigger sending message to USER_1's mailbox
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            tester_pid,
            // random params in ReplyDetails, because they aren't checked
            Kind::ReplyDetails(H256::random().into(), 0).encode(),
            50_000_000_000,
            0,
        ));
        run_to_next_block(None);

        let reply_to = get_last_mail(USER_1);
        assert_eq!(reply_to.id(), expected_mid, "mailbox check failed");

        let mp = MessageParamsBuilder::new(
            Kind::ReplyDetails(expected_mid.into_origin().into(), 0).encode(),
        )
        .with_reply_id(reply_to.id());

        (TestCall::send_reply(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_block_height() {
    run_tester(|_, _| {
        run_to_block(10, None);
        let expected_height = 11;

        let mp = Kind::BlockHeight(expected_height).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_block_timestamp() {
    run_tester(|_, _| {
        // will remain constant
        let block_timestamp = 125;
        assert_ok!(Timestamp::set(RuntimeOrigin::none(), block_timestamp));

        let mp = Kind::BlockTimestamp(block_timestamp).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

#[test]
fn check_gr_origin() {
    run_tester(|tester_id, _| {
        use demo_proxy::{InputArgs, WASM_BINARY as PROXY_WASM_BINARY};

        let payload = Kind::Origin(USER_2.into_origin().into()).encode();

        // Upload proxy
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: tester_id.into_origin().into()
            }
            .encode(),
            50_000_000_000,
            0
        ));
        let proxy_pid = get_last_program_id();
        run_to_next_block(None);

        // Set origin in the tester program through origin
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            proxy_pid,
            payload.clone(),
            50_000_000_000,
            0
        ));
        run_to_next_block(None);

        // Check the origin
        let mp = MessageParamsBuilder::new(payload).with_sender(USER_2);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_reserve_gas() {
    run_tester(|_, _| {
        let next_user_mid = get_next_message_id(USER_1);
        // Nonce in program is set to 2 due to 3 times reservation is called.
        let expected_reservation_id = ReservationId::generate(next_user_mid, 2).encode();
        let mp = Kind::Reserve(expected_reservation_id).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_unreserve_gas() {
    run_tester(|_, _| {
        let mp = Kind::Unreserve(10_000).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_random() {
    run_tester(|_, _| {
        run_to_block(10, None);

        let bn = 11;
        let salt = vec![1, 2, 3];
        let expected_hash = {
            let next_user_mid: [u8; 32] = get_next_message_id(USER_1).into();
            // Internals of the gr_random call
            let mut salt_clone = salt.clone();
            salt_clone.extend_from_slice(&next_user_mid);

            hash(&salt_clone)
        };

        let mp = Kind::Random(salt, (expected_hash, bn)).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_gas_available() {
    run_tester(|_, _| {
        // Expected to burn not more than 600_000_000
        // Provided gas in the test by default is 50_000_000_000
        let lower = 50_000_000_000 - 600_000_000;
        let upper = 50_000_000_000 - 300_000_000;
        let mp = Kind::GasAvailable(lower, upper).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

type DefaultPostCheck = fn() -> ();

enum TestCall {
    SendMessage(SendMessageParams),
    SendReply(SendReplyParams),
}

impl TestCall {
    fn send_message(mp: MessageParamsBuilder) -> Self {
        TestCall::SendMessage(mp.build_send_message())
    }

    fn send_reply(mp: MessageParamsBuilder) -> Self {
        TestCall::SendReply(mp.build_send_reply())
    }
}

struct SendMessageParams {
    sender: AccountId,
    payload: Vec<u8>,
    value: u128,
}

struct SendReplyParams {
    sender: AccountId,
    reply_to_id: MessageId,
    payload: Vec<u8>,
    value: u128,
}

#[derive(Default)]
struct MessageParamsBuilder {
    sender: Option<AccountId>,
    payload: Vec<u8>,
    value: Option<u128>,
    reply_to_id: Option<MessageId>,
}

impl MessageParamsBuilder {
    fn new(payload: Vec<u8>) -> Self {
        Self {
            payload,
            ..Default::default()
        }
    }

    fn with_sender(mut self, sender: AccountId) -> Self {
        self.sender = Some(sender);
        self
    }

    fn with_value(mut self, value: u128) -> Self {
        self.value = Some(value);
        self
    }

    fn with_reply_id(mut self, reply_to_id: MessageId) -> Self {
        self.reply_to_id = Some(reply_to_id);
        self
    }

    fn build_send_message(self) -> SendMessageParams {
        let MessageParamsBuilder {
            sender,
            payload,
            value,
            ..
        } = self;
        SendMessageParams {
            sender: sender.unwrap_or(USER_1),
            payload,
            value: value.unwrap_or(0),
        }
    }

    fn build_send_reply(self) -> SendReplyParams {
        let MessageParamsBuilder {
            sender,
            payload,
            value,
            reply_to_id,
        } = self;
        SendReplyParams {
            sender: sender.unwrap_or(USER_1),
            reply_to_id: reply_to_id.expect("internal error: reply id wasn't set"),
            payload,
            value: value.unwrap_or(0),
        }
    }
}

impl From<Vec<u8>> for MessageParamsBuilder {
    fn from(v: Vec<u8>) -> Self {
        MessageParamsBuilder::new(v)
    }
}

fn run_tester<P, S>(get_test_call_params: S)
where
    // Post check
    P: FnOnce(),
    // Get sys call and post check
    S: FnOnce(ProgramId, CodeId) -> (TestCall, Option<P>),
{
    init_logger();
    new_test_ext().execute_with(|| {
        let child_code = ProgramCodeKind::Default.to_bytes();
        let child_code_hash = generate_code_hash(&child_code);
        let tester_pid = generate_program_id(SYS_CALLS_TESTER_WASM_BINARY, DEFAULT_SALT);
        // let proxy_pid = generate_program_id(PROXY_WASM_BINARY, DEFAULT_SALT);

        // Deploy program with valid code hash
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        ));

        // Set default code-hash for create program calls
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            SYS_CALLS_TESTER_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            child_code_hash.encode(),
            50_000_000_000,
            0,
        ));

        run_to_next_block(None);

        let (call, post_check) = get_test_call_params(tester_pid, child_code_hash.into());
        match call {
            TestCall::SendMessage(mp) => {
                assert_ok!(Gear::send_message(
                    RuntimeOrigin::signed(mp.sender),
                    tester_pid,
                    mp.payload,
                    50_000_000_000,
                    mp.value,
                ));
            }
            TestCall::SendReply(rp) => {
                assert_ok!(Gear::send_reply(
                    RuntimeOrigin::signed(rp.sender),
                    rp.reply_to_id,
                    rp.payload,
                    50_000_000_000,
                    rp.value,
                ));
            }
        }

        // Main check
        let user_mid = get_last_message_id();
        run_to_next_block(None);
        assert_eq!(dispatch_status(user_mid), Some(DispatchStatus::Success));

        // Optional post-main check
        if let Some(post_check) = post_check {
            post_check();
        }
    })
}
