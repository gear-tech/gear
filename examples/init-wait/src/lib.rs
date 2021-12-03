#![no_std]

use gstd::{exec, msg, MessageId};

#[derive(PartialEq, Debug)]
enum State {
    NotInited,
    WaitForReply,
    Inited,
}

static mut STATE: State = State::NotInited;
static mut INIT_MESSAGE: MessageId = MessageId::new([0; 32]);

#[no_mangle]
pub unsafe extern "C" fn handle() {
    if STATE != State::Inited {
        panic!("not initialized");
    }

    msg::reply(b"Hello world!", 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    match STATE {
        State::NotInited => {
            INIT_MESSAGE = msg::id();
            msg::send(
                msg::source(),
                b"PING",
                exec::gas_available() - 100_000_000,
                0,
            );
            STATE = State::WaitForReply;
            exec::wait();
        }
        State::WaitForReply => {
            STATE = State::Inited;
        }
        _ => panic!("unreachable!"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    if STATE == State::WaitForReply {
        exec::wake(INIT_MESSAGE);
    }
}
