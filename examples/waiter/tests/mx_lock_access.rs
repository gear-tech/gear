use demo_waiter::{Command, LockContinuation, LockStaticAccessSubcommand, MxLockContinuation};
use gear_core::ids::MessageId;
use gtest::{Program, System, constants::DEFAULT_USER_ALICE};

pub const USER_ID: u64 = DEFAULT_USER_ALICE;

#[test]
fn drop_mx_lock_guard_from_different_msg_fails() {
    access_mx_lock_guard_from_different_msg_fails(LockStaticAccessSubcommand::Drop);
}

#[test]
fn as_ref_mx_lock_guard_from_different_msg_fails() {
    access_mx_lock_guard_from_different_msg_fails(LockStaticAccessSubcommand::AsRef);
}

#[test]
fn as_mut_mx_lock_guard_from_different_msg_fails() {
    access_mx_lock_guard_from_different_msg_fails(LockStaticAccessSubcommand::AsMut);
}

#[test]
fn deref_mx_lock_guard_from_different_msg_fails() {
    access_mx_lock_guard_from_different_msg_fails(LockStaticAccessSubcommand::Deref);
}

#[test]
fn deref_mut_mx_lock_guard_from_different_msg_fails() {
    access_mx_lock_guard_from_different_msg_fails(LockStaticAccessSubcommand::DerefMut);
}

fn access_mx_lock_guard_from_different_msg_fails(
    lock_access_subcommand: LockStaticAccessSubcommand,
) {
    let system = System::new();
    let (program, lock_msg_id) = init_fixture(&system);

    let lock_access_msg_id =
        program.send(USER_ID, Command::MxLockStaticAccess(lock_access_subcommand));
    let lock_access_result = system.run_next_block();

    lock_access_result.assert_panicked_with(lock_access_msg_id, format!(
        "Mutex guard held by message {lock_msg_id} is being accessed by message {lock_access_msg_id}"
    ));
}

fn init_fixture(system: &System) -> (Program<'_>, MessageId) {
    system.init_logger_with_default_filter("");
    let program = Program::current(system);
    program.send_bytes(USER_ID, []);
    let msg_id = program.send(
        USER_ID,
        Command::MxLock(
            None,
            MxLockContinuation::General(LockContinuation::MoveToStatic),
        ),
    );
    system.run_next_block();
    (program, msg_id)
}
