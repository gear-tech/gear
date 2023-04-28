use crate::SendingRequest;
use gstd::msg;

#[no_mangle]
extern "C" fn handle() {
    let SendingRequest {
        account_id,
        gas_limit,
        value,
    } = msg::load().expect("Failed to decode request");

    if let Some(gas_limit) = gas_limit {
        msg::send_bytes_with_gas(account_id, [], gas_limit, value)
            .expect("Failed to send gasful message");
    } else {
        msg::send_bytes(account_id, [], value).expect("Failed to send gasless message");
    }
}
