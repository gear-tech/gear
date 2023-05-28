use crate::{Log, Program, System};
use codec::Encode;
use core_processor::common::JournalHandler;
use gear_common::scheduler::TaskHandler;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, MessageWaitedType, Payload, StoredDispatch},
};

#[test]
fn pause_program_works() {
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
    sys.pause_program(program.id());

    // send a message to the program
    let res = program.send_bytes(0, b"PING");

    // check that the message was not processed, bc the program is paused
    assert_eq!(0, res.total_processed());
    assert_eq!(0, res.main_gas_burned().0);
    assert_eq!(0, res.others_gas_burned().0);
}

#[test]
#[should_panic]
fn pause_unknown_program_fails() {
    let sys = System::new();

    sys.pause_program(ProgramId::from(1337));
}

#[test]
fn mailbox_message_removal_works() {
    // Arranging data for future messages
    let sys = System::new();
    let message_id: MessageId = Default::default();
    let source_user_id = ProgramId::from(100);
    let destination_user_id = ProgramId::from(200);
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
    sys.send_dispatch(Dispatch::new(DispatchKind::Handle, message));

    // Getting mailbox of destination user
    let destination_user_mailbox = sys.get_mailbox(destination_user_id);

    // remove the message from the mailbox
    sys.remove_from_mailbox(destination_user_id, message_id);

    // Making sure that taken message is deleted
    assert!(!destination_user_mailbox.contains(&log))
}

#[test]
#[should_panic]
fn mailbox_message_removal_fails_on_wrong_user_id() {
    let sys = System::new();

    sys.remove_from_mailbox(1337, MessageId::default())
}

#[test]
fn waitlist_removal_works() {
    // initialize everything
    let sys = System::new();
    sys.init_logger();

    let mut manager = sys.0.borrow_mut();

    // add a dispatch to the waitlist
    let dispatch = StoredDispatch::new(DispatchKind::Handle, Message::default().into(), None);
    let duration = Some(64);
    let waited_type = MessageWaitedType::WaitUpTo;

    manager.wait_dispatch(dispatch.clone(), duration, waited_type);

    // check that waitlist contains the dispatch
    assert_eq!(manager.wait_list.len(), 1);
    assert_eq!(
        manager
            .wait_list
            .first_entry()
            .expect("failed to get first entry")
            .get(),
        &dispatch
    );

    // remove the message from the waitlist
    manager.remove_from_waitlist(ProgramId::default(), MessageId::default());

    // check that the waitlist is empty
    assert!(manager.wait_list.is_empty());
}

#[test]
#[should_panic]
fn waitlist_fails_on_unknown_ids() {
    let sys = System::new();

    sys.remove_from_waitlist(ProgramId::default(), MessageId::default());
}

#[test]
#[should_panic]
fn wake_message_fails_on_unknown_id() {
    let sys = System::new();

    sys.wake_message(ProgramId::default(), MessageId::default());
}
