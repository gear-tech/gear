use crate::{Arg, Calls, Scheme};
use parity_scale_codec::Encode;

pub const DEAD_MESSAGE: &[u8] = b"If you read this, I'm dead";
pub const UNREACHABLE_MESSAGE: &[u8] = b"UNREACHABLE";

pub fn init() -> Calls {
    let source_var = "source_var";
    let payload_var = "payload_var";
    let equity_var = "equity_var";

    Calls::builder()
        // Storing source id under `source_var`.
        .source(source_var)
        // Storing payload under `payload_var`.
        .load(payload_var)
        // Storing bool defining payload equals encoded "true" under `equity_var`.
        .bytes_eq(equity_var, payload_var, Arg::bytes(true.encode()))
        // Branching logic dependent on equity result.
        // Sends dead message in true case, otherwise noop.
        .if_else(
            equity_var,
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

pub fn scheme() -> Scheme {
    Scheme::predefined(init(), handle(), handle_reply())
}
