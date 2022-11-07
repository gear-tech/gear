#![allow(unused)]

use super::{
    assert_ok, pallet, run_to_block, Event, MailboxOf, MockRuntimeEvent, RuntimeOrigin, Test,
};
use crate::{
    mock::{Balances, Gear, System},
    BalanceOf, GasInfo, HandleKind, SentOf,
};
use codec::Decode;
use common::{
    event::*,
    storage::{CountedByKey, Counter, IterableByKeyMap},
    Origin,
};
use core_processor::common::ExecutionErrorReason;
use frame_support::{
    dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
    traits::tokens::{currency::Currency, Balance},
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
use gear_backend_common::TrapExplanation;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    message::StoredMessage,
    reservation::GasReservationMap,
};
use gear_core_errors::ExtError;
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{convert::TryFrom, fmt::Debug};

pub(super) const DEFAULT_GAS_LIMIT: u64 = 200_000_000;
pub(super) const DEFAULT_SALT: &[u8; 4] = b"salt";
pub(super) const EMPTY_PAYLOAD: &[u8; 0] = b"";
pub(super) const OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS: u64 = 10000000;

pub(super) type DispatchCustomResult<T> = Result<T, DispatchErrorWithPostInfo>;
pub(super) type AccountId = <Test as frame_system::Config>::AccountId;
pub(super) type GasPrice = <Test as pallet::Config>::GasPrice;

type BlockNumber = <Test as frame_system::Config>::BlockNumber;

pub(super) fn hash(data: impl AsRef<[u8]>) -> [u8; 32] {
    sp_core::blake2_256(data.as_ref())
}

pub(super) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

pub(super) fn assert_balance(
    origin: impl common::Origin,
    free: impl Into<BalanceOf<Test>>,
    reserved: impl Into<BalanceOf<Test>>,
) {
    let account_id = AccountId::from_origin(origin.into_origin());
    assert_eq!(Balances::free_balance(account_id), free.into());
    assert_eq!(Balances::reserved_balance(account_id), reserved.into());
}

pub(super) fn calculate_handle_and_send_with_extra(
    origin: AccountId,
    destination: ProgramId,
    payload: Vec<u8>,
    gas_limit: Option<u64>,
    value: BalanceOf<Test>,
) -> (MessageId, GasInfo) {
    let gas_info = Gear::calculate_gas_info(
        origin.into_origin(),
        HandleKind::Handle(destination),
        payload.clone(),
        value,
        true,
    )
    .expect("calculate_gas_info failed");

    let limit = gas_info.min_limit + gas_limit.unwrap_or_default();

    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(origin),
        destination,
        payload,
        limit,
        value
    ));

    let message_id = get_last_message_id();

    (message_id, gas_info)
}

pub(super) fn get_ed() -> u128 {
    <Test as pallet::Config>::Currency::minimum_balance().unique_saturated_into()
}

pub(super) fn assert_init_success(expected: u32) {
    let mut actual_children_amount = 0;
    System::events().iter().for_each(|e| {
        if let MockRuntimeEvent::Gear(Event::ProgramChanged {
            change: ProgramChangeKind::Active { .. },
            ..
        }) = e.event
        {
            actual_children_amount += 1
        }
    });

    assert_eq!(expected, actual_children_amount);
}

pub(super) fn assert_last_dequeued(expected: u32) {
    let last_dequeued = System::events()
        .iter()
        .filter_map(|e| {
            if let MockRuntimeEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                Some(total)
            } else {
                None
            }
        })
        .last()
        .expect("Not found RuntimeEvent::MessagesDispatched");

    assert_eq!(expected, last_dequeued);
}

pub(super) fn assert_total_dequeued(expected: u32) {
    let actual_dequeued: u32 = System::events()
        .iter()
        .filter_map(|e| {
            if let MockRuntimeEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                Some(total)
            } else {
                None
            }
        })
        .sum();

    assert_eq!(expected, actual_dequeued);
}

// Creates a new program and puts message from program to `user` in mailbox
// using extrinsic calls. Imitates real-world sequence of calls.
//
// *NOTE*:
// 1) usually called inside first block
// 2) runs to block 2 all the messages place to message queue/storage
//
// Returns id of the message in the mailbox
pub(super) fn setup_mailbox_test_state(user: AccountId) -> MessageId {
    let prog_id = {
        let res = upload_program_default(user, ProgramCodeKind::OutgoingWithValueInHandle);
        assert_ok!(res);
        res.expect("submit result was asserted")
    };

    increase_prog_balance_for_mailbox_test(user, prog_id);
    populate_mailbox_from_program(prog_id, user, 2, 2_000_000_000, 0)
}

// Puts message from `prog_id` for the `user` in mailbox and returns its id
pub(super) fn populate_mailbox_from_program(
    prog_id: ProgramId,
    sender: AccountId,
    block_num: BlockNumber,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(sender),
        prog_id,
        Vec::new(),
        gas_limit, // `prog_id` program sends message in handle which sets gas limit to 10_000_000.
        value,
    ));

    let message_id = get_last_message_id();
    run_to_block(block_num, None);

    {
        let expected_code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
        assert_eq!(
            common::get_program(prog_id.into_origin())
                .and_then(|p| common::ActiveProgram::try_from(p).ok())
                .expect("program must exist")
                .code_hash,
            generate_code_hash(&expected_code).into(),
            "can invoke send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
        );
    }

    MessageId::generate_outgoing(message_id, 0)
}

pub(super) fn increase_prog_balance_for_mailbox_test(sender: AccountId, program_id: ProgramId) {
    let expected_code_hash: H256 = generate_code_hash(
        ProgramCodeKind::OutgoingWithValueInHandle
            .to_bytes()
            .as_slice(),
    )
    .into();
    let actual_code_hash = common::get_program(program_id.into_origin())
        .and_then(|p| common::ActiveProgram::try_from(p).ok())
        .map(|prog| prog.code_hash)
        .expect("invalid program address for the test");
    assert_eq!(
        expected_code_hash, actual_code_hash,
        "invalid program code for the test"
    );

    // This value is actually a constants in `ProgramCodeKind::OutgoingWithValueInHandle` wat. Alternatively can be read from Mailbox.
    let locked_value = 1000;

    // When program sends message, message value (if not 0) is reserved.
    // If value can't be reserved, message is skipped.
    assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
        &sender,
        &AccountId::from_origin(program_id.into_origin()),
        locked_value,
        frame_support::traits::ExistenceRequirement::AllowDeath
    ));
}

// Submits program with default options (salt, gas limit, value, payload)
pub(super) fn upload_program_default(
    user: AccountId,
    code_kind: ProgramCodeKind,
) -> DispatchCustomResult<ProgramId> {
    let code = code_kind.to_bytes();
    let salt = DEFAULT_SALT.to_vec();

    Gear::upload_program(
        RuntimeOrigin::signed(user),
        code,
        salt,
        EMPTY_PAYLOAD.to_vec(),
        DEFAULT_GAS_LIMIT,
        0,
    )
    .map(|_| get_last_program_id())
}

pub(super) fn generate_program_id(code: &[u8], salt: &[u8]) -> ProgramId {
    ProgramId::generate(CodeId::generate(code), salt)
}

pub(super) fn generate_code_hash(code: &[u8]) -> [u8; 32] {
    CodeId::generate(code).into()
}

/// Gets next message id, but doesn't remain changed the state of the nonces
pub(super) fn get_next_message_id(user_id: impl Origin) -> MessageId {
    let ret_id = Gear::next_message_id(user_id.into_origin());
    SentOf::<Test>::decrease();
    ret_id
}

pub(super) fn send_default_message(from: AccountId, to: ProgramId) -> DispatchResultWithPostInfo {
    Gear::send_message(
        RuntimeOrigin::signed(from),
        to,
        EMPTY_PAYLOAD.to_vec(),
        DEFAULT_GAS_LIMIT,
        0,
    )
}

pub(super) fn call_default_message(to: ProgramId) -> crate::mock::RuntimeCall {
    crate::mock::RuntimeCall::Gear(crate::Call::<Test>::send_message {
        destination: to,
        payload: EMPTY_PAYLOAD.to_vec(),
        gas_limit: DEFAULT_GAS_LIMIT,
        value: 0,
    })
}

pub(super) fn dispatch_status(message_id: MessageId) -> Option<DispatchStatus> {
    let mut found_status: Option<DispatchStatus> = None;
    System::events().iter().for_each(|e| {
        if let MockRuntimeEvent::Gear(Event::MessagesDispatched { statuses, .. }) = &e.event {
            found_status = statuses.get(&message_id).map(Clone::clone);
        }
    });

    found_status
}

pub(super) fn assert_dispatched(message_id: MessageId) {
    assert!(dispatch_status(message_id).is_some())
}

pub(super) fn assert_succeed(message_id: MessageId) {
    let status =
        dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

    assert_eq!(status, DispatchStatus::Success)
}

pub(super) fn assert_failed(message_id: MessageId, error: ExecutionErrorReason) {
    let status =
        dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

    assert_eq!(status, DispatchStatus::Failed, "Expected: {error}");

    let mut actual_error = None;

    System::events().into_iter().for_each(|e| {
        if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
            if let Some(details) = message.reply() {
                if details.reply_to() == message_id && details.exit_code() != 0 {
                    actual_error = Some(
                        String::from_utf8(message.payload().to_vec())
                            .expect("Unable to decode string from error reply"),
                    );
                }
            }
        }
    });

    let mut actual_error =
        actual_error.expect("Error message not found in any `RuntimeEvent::UserMessageSent`");
    let mut expectations = error.to_string();

    log::debug!("Actual error: {:?}", actual_error);

    // In many cases fallible syscall returns ExtError, which program unwraps afterwards.
    // This check handles display of the error inside.
    if actual_error.starts_with('\'') {
        let j = actual_error.rfind('\'').expect("Checked above");
        actual_error = String::from(&actual_error[..(j + 1)]);
        expectations = format!("'{expectations}'");
    }

    assert_eq!(expectations, actual_error)
}

pub(super) fn assert_not_executed(message_id: MessageId) {
    let status =
        dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

    assert_eq!(status, DispatchStatus::NotExecuted)
}

pub(super) fn get_last_event() -> MockRuntimeEvent {
    System::events()
        .into_iter()
        .last()
        .expect("failed to get last event")
        .event
}

pub(super) fn get_last_program_id() -> ProgramId {
    let event = match System::events().last().map(|r| r.event.clone()) {
        Some(MockRuntimeEvent::Gear(e)) => e,
        _ => unreachable!("Should be one Gear event"),
    };

    if let Event::MessageEnqueued {
        destination,
        entry: Entry::Init,
        ..
    } = event
    {
        destination
    } else {
        unreachable!("expect RuntimeEvent::InitMessageEnqueued")
    }
}

pub(super) fn get_last_code_id() -> CodeId {
    let event = match System::events().last().map(|r| r.event.clone()) {
        Some(MockRuntimeEvent::Gear(e)) => e,
        _ => unreachable!("Should be one Gear event"),
    };

    if let Event::CodeChanged {
        change: CodeChangeKind::Active { .. },
        id,
        ..
    } = event
    {
        id
    } else {
        unreachable!("expect Event::CodeChanged")
    }
}

pub(super) fn filter_event_rev<F, R>(f: F) -> R
where
    F: Fn(Event<Test>) -> Option<R>,
{
    System::events()
        .iter()
        .rev()
        .filter_map(|r| {
            if let MockRuntimeEvent::Gear(e) = r.event.clone() {
                Some(e)
            } else {
                None
            }
        })
        .find_map(f)
        .expect("can't find message send event")
}

pub(super) fn get_last_message_id() -> MessageId {
    System::events()
        .iter()
        .rev()
        .filter_map(|r| {
            if let MockRuntimeEvent::Gear(e) = r.event.clone() {
                Some(e)
            } else {
                None
            }
        })
        .find_map(|e| match e {
            Event::MessageEnqueued { id, .. } => Some(id),
            Event::UserMessageSent { message, .. } => Some(message.id()),
            _ => None,
        })
        .expect("can't find message send event")
}

pub(super) fn get_waitlist_expiration(message_id: MessageId) -> BlockNumberFor<Test> {
    let mut exp = None;
    System::events()
        .into_iter()
        .rfind(|e| match e.event {
            MockRuntimeEvent::Gear(Event::MessageWaited {
                id: message_id,
                expiration,
                ..
            }) => {
                exp = Some(expiration);
                true
            }
            _ => false,
        })
        .expect("Failed to find appropriate MessageWaited event");

    exp.unwrap()
}

pub(super) fn get_last_message_waited() -> (MessageId, BlockNumberFor<Test>) {
    let mut message_id = None;
    let mut exp = None;
    System::events()
        .into_iter()
        .rfind(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited { id, expiration, .. }) = e.event {
                message_id = Some(id);
                exp = Some(expiration);
                true
            } else {
                false
            }
        })
        .expect("Failed to find appropriate MessageWaited event");

    (message_id.unwrap(), exp.unwrap())
}

pub(super) fn maybe_last_message(account: AccountId) -> Option<StoredMessage> {
    System::events().into_iter().rev().find_map(|e| {
        if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
            if message.destination() == account.into() {
                Some(message)
            } else {
                None
            }
        } else {
            None
        }
    })
}

pub(super) fn get_last_mail(account: AccountId) -> StoredMessage {
    MailboxOf::<Test>::iter_key(account)
        .last()
        .map(|(msg, _bn)| msg)
        .expect("Element should be")
}

pub(super) fn get_reservation_map(pid: ProgramId) -> Option<GasReservationMap> {
    let prog = common::get_program(pid.into_origin()).unwrap();
    if let common::Program::Active(common::ActiveProgram {
        gas_reservation_map,
        ..
    }) = prog
    {
        Some(gas_reservation_map)
    } else {
        None
    }
}

#[derive(Debug, Copy, Clone)]
pub(super) enum ProgramCodeKind<'a> {
    Default,
    Custom(&'a str),
    GreedyInit,
    OutgoingWithValueInHandle,
}

impl<'a> ProgramCodeKind<'a> {
    pub(super) fn to_bytes(self) -> Vec<u8> {
        let source = match self {
            ProgramCodeKind::Default => {
                r#"
                (module
                    (import "env" "memory" (memory 1))
                    (export "handle" (func $handle))
                    (export "init" (func $init))
                    (func $handle)
                    (func $init)
                )"#
            }
            ProgramCodeKind::GreedyInit => {
                // Initialization function for that program requires a lot of gas.
                // So, providing `DEFAULT_GAS_LIMIT` will end up processing with
                // "Not enough gas to continue execution" a.k.a. "Gas limit exceeded"
                // execution outcome error message.
                r#"
                (module
                    (import "env" "memory" (memory 1))
                    (export "init" (func $init))
                    (func $doWork (param $size i32)
                        (local $counter i32)
                        i32.const 0
                        set_local $counter
                        loop $while
                            get_local $counter
                            i32.const 1
                            i32.add
                            set_local $counter
                            get_local $counter
                            get_local $size
                            i32.lt_s
                            if
                                br $while
                            end
                        end $while
                    )
                    (func $init
                        i32.const 0x7fff_ffff
                        call $doWork
                    )
                )"#
            }
            ProgramCodeKind::OutgoingWithValueInHandle => {
                // Sending message to USER_1 is hardcoded!
                // Program sends message in handle which sets gas limit to 10_000_000 and value to 1000.
                // [warning] - program payload data is inaccurate, don't make assumptions about it!
                r#"
                (module
                    (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32 i32) (result i32)))
                    (import "env" "gr_source" (func $gr_source (param i32)))
                    (import "env" "memory" (memory 1))
                    (export "handle" (func $handle))
                    (export "init" (func $init))
                    (export "handle_reply" (func $handle_reply))
                    (func $handle
                        (local $msg_source i32)
                        (local $msg_val i32)
                        (i32.store offset=2
                            (get_local $msg_source)
                            (i32.const 1)
                        )
                        (i32.store offset=10
                            (get_local $msg_val)
                            (i32.const 1000)
                        )
                        (call $send (i32.const 2) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 10) (i32.const 0) (i32.const 40000))
                        (if
                            (then unreachable)
                            (else)
                        )
                    )
                    (func $handle_reply)
                    (func $init)
                )"#
            }
            ProgramCodeKind::Custom(code) => code,
        };

        wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec()
    }
}

pub(super) fn print_gear_events() {
    let v = System::events()
        .into_iter()
        .map(|r| r.event)
        .collect::<Vec<_>>();

    println!("Gear events");
    for (pos, line) in v.iter().enumerate() {
        println!("{pos}). {line:?}");
    }
}

pub(super) fn waiting_init_messages(pid: ProgramId) -> Vec<MessageId> {
    let key = common::waiting_init_prefix(pid);
    sp_io::storage::get(&key)
        .and_then(|v| Vec::<MessageId>::decode(&mut &v[..]).ok())
        .unwrap_or_default()
}
