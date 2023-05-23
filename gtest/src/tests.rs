use crate::{program::ProgramIdWrapper, Log, Program, System};
use codec::Encode;
use gear_common::scheduler::TaskHandler;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, Payload},
};

#[test]
fn program_pause_works() {
    // initialize everything
    let sys = System::new();

    sys.init_logger();

    let program = Program::from_file(
        &sys,
        "../target/wasm32-unknown-unknown/release/demo_ping.opt.wasm",
    );

    // send a message to the program
    let res = program.send_bytes(0, b"PING");
    // check that the message was processed, bc the program is not paused
    assert_eq!(1, res.total_processed());

    // pause the program
    // TODO: convenience method for this
    program
        .manager
        .try_borrow_mut()
        .expect("failed to borrow manager")
        .pause_program(program.id());

    // send a message to the program
    let res = program.send_bytes(0, b"PING");
    // check that the message was not processed, bc the program is paused
    assert_eq!(0, res.total_processed());
    assert_eq!(0, res.main_gas_burned().0);
    assert_eq!(0, res.others_gas_burned().0);
}

#[test]
fn mailbox_message_removal_works() {
    // Arranging data for future messages
    const SOURCE_USER_ID: u64 = 100;
    const DESTINATION_USER_ID: u64 = 200;
    let system = System::new();
    let message_id: MessageId = Default::default();
    let source_user_id = ProgramId::from(SOURCE_USER_ID);
    let destination_user_id = ProgramId::from(DESTINATION_USER_ID);
    let message_payload: Payload = vec![1, 2, 3].try_into().expect("failed to encode payload");
    let log = Log::builder().payload(message_payload.clone());

    // Building message based on arranged data
    let message = Message::new(
        message_id,
        source_user_id,
        destination_user_id,
        message_payload.encode().try_into().unwrap(),
        Default::default(),
        0,
        None,
    );

    // Sending created message
    let res = system.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

    // Getting mailbox of destination user
    let destination_user_mailbox = system.get_mailbox(destination_user_id);

    // remove the message from the mailbox
    // TODO: convenience method for this
    system
        .0
        .try_borrow_mut()
        .expect("failed to borrow manager")
        .remove_from_mailbox(DESTINATION_USER_ID, message_id);

    // Making sure that taken message is deleted
    assert!(!destination_user_mailbox.contains(&log))
}

#[test]
fn mailbox_message_removal_fails_if_wrong_user_id() {} // TODO: consider naming

#[test]
fn mailbox_message_removal_fails_if_wrong_message_id() {} // TODO: consider naming

#[test]
#[should_panic]
fn pause_of_unknown_program_fails() {}
