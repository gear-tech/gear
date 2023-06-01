use crate::{Arg, Calls, Scheme};

pub const DEAD_MESSAGE: &[u8] = b"If you read this, I'm dead";
pub const UNREACHABLE_MESSAGE: &[u8] = b"UNREACHABLE";

pub fn init(send_before_exit: bool) -> Calls {
    let source_var = "source_var";

    Calls::builder()
        // Storing source id under `source_var`.
        .source(source_var)
        // Sends dead message if `send_before_exit`.
        .if_else(
            send_before_exit,
            Calls::builder().send(source_var, Arg::bytes(DEAD_MESSAGE)),
            Calls::builder().noop(),
        )
        // Exit call.
        .exit(source_var)
        // Extra send that could never be executed.
        .send(source_var, Arg::bytes(UNREACHABLE_MESSAGE))
}

pub fn handle() -> Calls {
    Calls::builder().noop()
}

pub fn handle_reply() -> Calls {
    Calls::builder().noop()
}

pub fn scheme(send_before_exit: bool) -> Scheme {
    Scheme::predefined(init(send_before_exit), handle(), handle_reply())
}
