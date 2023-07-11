use demo_waiter::{
    Command, LockContinuation, LockStaticAccessSubcommand, RwLockContinuation, RwLockType,
};
use gear_core::ids::MessageId;
use gtest::{Program, System};
use utils::{assert_paniced, USER_ID};

mod utils;

#[test]
fn drop_r_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Read);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Read, LockStaticAccessSubcommand::Drop),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Read lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn as_ref_r_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Read);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Read, LockStaticAccessSubcommand::AsRef),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Read lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn deref_r_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Read);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Read, LockStaticAccessSubcommand::Deref),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Read lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn drop_w_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Write);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Write, LockStaticAccessSubcommand::Drop),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Write lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn as_ref_w_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Write);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Write, LockStaticAccessSubcommand::AsRef),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Write lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn as_mut_w_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Write);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Write, LockStaticAccessSubcommand::AsMut),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Write lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn deref_w_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Write);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Write, LockStaticAccessSubcommand::Deref),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Write lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn deref_mut_w_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, RwLockType::Write);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(RwLockType::Write, LockStaticAccessSubcommand::DerefMut),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Write lock guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

fn init_fixture(system: &System, lock_type: RwLockType) -> (Program<'_>, MessageId) {
    system.init_logger_with_default_filter("");
    let program = Program::current(system);
    program.send_bytes(USER_ID, []);
    let lock_result = program.send(
        USER_ID,
        Command::RwLock(
            lock_type,
            RwLockContinuation::General(LockContinuation::MoveToStatic),
        ),
    );
    (program, lock_result.sent_message_id())
}
