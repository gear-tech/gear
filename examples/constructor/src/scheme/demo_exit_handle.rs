use crate::{Arg, Calls, Scheme};

pub const UNREACHABLE_MESSAGE: &[u8] = b"UNREACHABLE";

pub fn init() -> Calls {
    Calls::builder().noop()
}

pub fn handle() -> Calls {
    let source_var = "source_var";

    Calls::builder()
        // Storing source id under `source_var`.
        .source(source_var)
        // Exit call.
        .exit(source_var)
        // Extra send that could never be executed.
        .send(source_var, Arg::bytes(UNREACHABLE_MESSAGE))
}

pub fn handle_reply() -> Calls {
    Calls::builder().noop()
}

pub fn scheme() -> Scheme {
    Scheme::predefined(init(), handle(), handle_reply())
}
