use crate::{Arg, Calls, Scheme};

pub const PING: &[u8] = b"PING";
pub const PONG: &[u8] = b"PONG";

pub fn init() -> Calls {
    Calls::builder().noop()
}

pub fn handle() -> Calls {
    let payload_var = "payload_var";
    let equity_var = "equity_var";

    Calls::builder()
        // Storing payload under `payload_var`.
        .load(payload_var)
        // Storing bool defining payload equals "PING" under `equity_var`.
        .bytes_eq(equity_var, payload_var, Arg::bytes(PING))
        // Branching logic dependent on equity result.
        // Sends "PONG" reply in true case, otherwise noop.
        .if_else(
            equity_var,
            Calls::builder().reply(Arg::bytes(PONG)),
            Calls::builder().noop(),
        )
}

pub fn handle_reply() -> Calls {
    Calls::builder().noop()
}

pub fn scheme() -> Scheme {
    Scheme::predefined(init(), handle(), handle_reply())
}
