
use gstd::{msg, ProgramId};

/// Send messagege and wait for reply.
pub async fn send_and_wait(program: ProgramId, payload: &[u8], gas_limit: u64, value: u128) {
    msg::send(program, payload, gas_limit, value);
}
