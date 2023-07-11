use demo_waiter::{Command, LockContinuation, LockStaticAccessSubcommand, MxLockContinuation};
use gear_core::ids::MessageId;
use gtest::{Program, System};
use utils::{assert_paniced, USER_ID};

mod utils;

#[test]
fn drop_mx_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_result = program.send(
        USER_ID,
        Command::MxLockStaticAccess(LockStaticAccessSubcommand::Drop),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Mutex guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn as_ref_mx_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_result = program.send(
        USER_ID,
        Command::MxLockStaticAccess(LockStaticAccessSubcommand::AsRef),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Mutex guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn as_mut_mx_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_result = program.send(
        USER_ID,
        Command::MxLockStaticAccess(LockStaticAccessSubcommand::AsMut),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Mutex guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn deref_mx_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_result = program.send(
        USER_ID,
        Command::MxLockStaticAccess(LockStaticAccessSubcommand::Deref),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Mutex guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

#[test]
fn deref_mut_mx_lock_guard_from_different_msg_fails() {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_result = program.send(
        USER_ID,
        Command::MxLockStaticAccess(LockStaticAccessSubcommand::DerefMut),
    );

    assert_paniced(
        &lock_access_result,
        &format!(
            "Mutex guard held by message {} is being accessed by message {}",
            lock_msg_id,
            lock_access_result.sent_message_id()
        ),
    );
}

fn init_fixture(system: &System) -> (Program<'_>, MessageId) {
    system.init_logger_with_default_filter("");
    let program = Program::current(system);
    program.send_bytes(USER_ID, []);
    let lock_result = program.send(
        USER_ID,
        Command::MxLock(MxLockContinuation::General(LockContinuation::MoveToStatic)),
    );
    (program, lock_result.sent_message_id())
}
