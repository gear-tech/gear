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
extern "C" fn handle() {
    if unsafe { STATE != State::Inited } {
        panic!("not initialized");
    }

    msg::reply(b"Hello, world!", 0).unwrap();
}

#[no_mangle]
extern "C" fn init() {
    let state = unsafe { &mut STATE };
    match state {
        State::NotInited => {
            for k in 0..20 {
                unsafe { TEST_DYNAMIC_MEMORY.insert(k, ()) };
            }

            unsafe { INIT_MESSAGE = msg::id() };
            msg::send(msg::source(), b"PING", 0).unwrap();
            *state = State::WaitForReply;
            exec::wait();
        }
        State::WaitForReply => {
            for k in 0..20 {
                unsafe { TEST_DYNAMIC_MEMORY.insert(k, ()) };
            }
            for k in 0..25 {
                let _ = unsafe { TEST_DYNAMIC_MEMORY.remove(&k) };
            }

            *state = State::Inited;
        }
        _ => panic!("unreachable!"),
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    if unsafe { STATE == State::WaitForReply } {
        for k in 20..40 {
            unsafe { TEST_DYNAMIC_MEMORY.insert(k, ()) };
        }
        exec::wake(unsafe { INIT_MESSAGE });
    }
}
