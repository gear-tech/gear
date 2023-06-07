use crate::{Arg, Call, Calls, Scheme};
use parity_scale_codec::Encode;

pub const PROXIED_MESSAGE: &[u8] = b"proxied message";

const DESTINATION_VAR: &str = "destination_var";
const GAS_LIMIT_VAR: &str = "gas_limit_var";
const VALUE_VAR: &str = "value_var";
const DELAY_VAR: &str = "delay_var";

fn send_call() -> Call {
    Call::Send(
        Arg::get(DESTINATION_VAR),
        Arg::bytes(PROXIED_MESSAGE),
        Some(Arg::get(GAS_LIMIT_VAR)),
        Arg::get(VALUE_VAR),
        Arg::get(DELAY_VAR),
    )
}

pub fn init(destination: [u8; 32], delay: u32) -> Calls {
    Calls::builder()
        // Storing destination under `DESTINATION_VAR`.
        .add_call(Call::Vec(destination.encode()))
        .store(DESTINATION_VAR)
        // Storing delay under `DELAY_VAR`.
        .add_call(Call::Vec(delay.encode()))
        .store(DELAY_VAR)
}

pub fn handle() -> Calls {
    Calls::builder()
        // Storing u64 from payload under `GAS_LIMIT_VAR`.
        .load_bytes(GAS_LIMIT_VAR)
        // Storing message value under `VALUE_VAR`.
        .value(VALUE_VAR)
        // Sending proxy message.
        .add_call(send_call())
}

pub fn handle_reply() -> Calls {
    let status_code_var = "status_code_var";
    let equity_var = "equity_var";

    Calls::builder()
        // Storing u64 from payload under `GAS_LIMIT_VAR`.
        .load_bytes(GAS_LIMIT_VAR)
        // Storing message value under `VALUE_VAR`.
        .value(VALUE_VAR)
        // Storing status code under `status_code_var`.
        .add_call(Call::StatusCode)
        .store_vec(status_code_var)
        // Branching logic related to status code.
        .bytes_eq(equity_var, status_code_var, 0i32.encode())
        // Sending proxy message.
        .if_else(
            equity_var,
            Calls::builder().add_call(send_call()),
            Calls::builder().noop(),
        )
}

pub fn scheme(destination: [u8; 32], delay: u32) -> Scheme {
    Scheme::predefined(init(destination, delay), handle(), handle_reply())
}
