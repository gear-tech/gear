use crate::{Arg, Call, Calls, Scheme};

pub const DESTINATION_MESSAGE: &[u8] = b"We are doing!";
pub const SUCCESS_MESSAGE: &[u8] = b"We are done!";

const GAS_LIMIT_VAR: &str = "gas_limit_var";
const MESSAGE_ID_VAR: &str = "message_id_var";

pub fn init() -> Calls {
    Calls::builder().noop()
}

pub fn handle(destination: [u8; 32], gas_to_send: u64) -> Calls {
    Calls::builder()
        // Sending message to pre-defined destination with 0 gas limit.
        .send_wgas(destination, Arg::bytes(DESTINATION_MESSAGE), gas_to_send)
        // Storing message id.
        .store(MESSAGE_ID_VAR)
        // Storing u64 from payload under `GAS_LIMIT_VAR`.
        .load_bytes(GAS_LIMIT_VAR)
        // Creating reply deposit.
        .add_call(Call::ReplyDeposit(
            MESSAGE_ID_VAR.into(),
            GAS_LIMIT_VAR.into(),
        ))
}

pub fn handle_reply(checker: [u8; 32]) -> Calls {
    Calls::builder()
        // Sending success message.
        .send_wgas(checker, Arg::bytes(SUCCESS_MESSAGE), 10_000)
}

pub fn scheme(checker: [u8; 32], destination: [u8; 32], gas_to_send: u64) -> Scheme {
    Scheme::predefined(
        init(),
        handle(destination, gas_to_send),
        handle_reply(checker),
    )
}
