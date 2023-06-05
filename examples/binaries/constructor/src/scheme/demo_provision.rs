use crate::{Arg, Call, Calls, Scheme};
use parity_scale_codec::Encode;

pub const SUCCESS_MESSAGE: &[u8] = b"We are done!";

const CHECKER_DESTINATION_VAR: &str = "checker_destination_var";
const DESTINATION_VAR: &str = "destination_var";
const GAS_LIMIT_VAR: &str = "gas_limit_var";
const MESSAGE_ID_VAR: &str = "message_id_var";

pub fn init(checker: [u8; 32], destination: [u8; 32]) -> Calls {
    Calls::builder()
        // Storing checker destination under `DESTINATION_VAR`.
        .add_call(Call::Vec(checker.encode()))
        .store(CHECKER_DESTINATION_VAR)
        // Storing destination under `DESTINATION_VAR`.
        .add_call(Call::Vec(destination.encode()))
        .store(DESTINATION_VAR)
}

pub fn handle() -> Calls {
    Calls::builder()
        // Sending message to pre-defined in init destination with 0 gas limit.
        .send_wgas(DESTINATION_VAR, [], 0)
        // Storing message id.
        .store(MESSAGE_ID_VAR)
        // Storing u64 from payload under `GAS_LIMIT_VAR`.
        .load_bytes(GAS_LIMIT_VAR)
        // Creating provision.
        .add_call(Call::CreateProvision(
            MESSAGE_ID_VAR.into(),
            GAS_LIMIT_VAR.into(),
        ))
}

pub fn handle_reply() -> Calls {
    Calls::builder()
        // Sending success message.
        .send_wgas(CHECKER_DESTINATION_VAR, Arg::bytes(SUCCESS_MESSAGE), 10_000)
}

pub fn scheme(checker: [u8; 32], destination: [u8; 32]) -> Scheme {
    Scheme::predefined(init(checker, destination), handle(), handle_reply())
}
