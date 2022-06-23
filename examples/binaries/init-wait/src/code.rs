use gstd::{exec, msg, BTreeMap, MessageId};

#[derive(PartialEq, Debug)]
enum State {
    NotInited,
    WaitForReply,
    Inited,
}

static mut STATE: State = State::NotInited;
static mut INIT_MESSAGE: MessageId = MessageId::new([0; 32]);
static mut TEST_DYNAMIC_MEMORY: BTreeMap<u32, ()> = BTreeMap::new();

#[no_mangle]
unsafe extern "C" fn handle() {
    if STATE != State::Inited {
        panic!("not initialized");
    }

    msg::reply(b"Hello, world!", 0).unwrap();
}

#[no_mangle]
unsafe extern "C" fn init() {
    match STATE {
        State::NotInited => {
            for k in 0..20 {
                TEST_DYNAMIC_MEMORY.insert(k, ());
            }

            INIT_MESSAGE = msg::id();
            msg::send(msg::source(), b"PING", 0).unwrap();
            STATE = State::WaitForReply;
            exec::wait();
        }
        State::WaitForReply => {
            for k in 0..20 {
                TEST_DYNAMIC_MEMORY.insert(k, ());
            }
            for k in 0..25 {
                let _ = TEST_DYNAMIC_MEMORY.remove(&k);
            }

            STATE = State::Inited;
        }
        _ => panic!("unreachable!"),
    }
}

#[no_mangle]
unsafe extern "C" fn handle_reply() {
    if STATE == State::WaitForReply {
        for k in 20..40 {
            TEST_DYNAMIC_MEMORY.insert(k, ());
        }
        exec::wake(INIT_MESSAGE);
    }
}
