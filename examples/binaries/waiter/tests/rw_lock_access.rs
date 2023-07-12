use demo_waiter::{
    Command, LockContinuation, LockStaticAccessSubcommand, RwLockContinuation, RwLockType,
};
use gear_core::ids::MessageId;
use gtest::{Program, System};
use utils::{assert_panicked, USER_ID};

mod utils;

#[test]
fn drop_r_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Read,
        LockStaticAccessSubcommand::Drop,
    );
}

#[test]
fn as_ref_r_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Read,
        LockStaticAccessSubcommand::AsRef,
    );
}

#[test]
fn deref_r_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Read,
        LockStaticAccessSubcommand::Deref,
    );
}

#[test]
fn drop_w_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Write,
        LockStaticAccessSubcommand::Drop,
    );
}

#[test]
fn as_ref_w_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Write,
        LockStaticAccessSubcommand::AsRef,
    );
}

#[test]
fn as_mut_w_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Write,
        LockStaticAccessSubcommand::AsMut,
    );
}

#[test]
fn deref_w_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Write,
        LockStaticAccessSubcommand::Deref,
    );
}

#[test]
fn deref_mut_w_lock_guard_from_different_msg_fails() {
    access_rw_lock_guard_from_different_msg_fails(
        RwLockType::Write,
        LockStaticAccessSubcommand::DerefMut,
    );
}

fn access_rw_lock_guard_from_different_msg_fails(
    lock_type: RwLockType,
    lock_access_subcommand: LockStaticAccessSubcommand,
) {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system, lock_type);

    let lock_access_result = program.send(
        USER_ID,
        Command::RwLockStaticAccess(lock_type, lock_access_subcommand),
    );

    assert_panicked(
        &lock_access_result,
        &format!(
            "{:?} lock guard held by message {} is being accessed by message {}",
            lock_type,
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
