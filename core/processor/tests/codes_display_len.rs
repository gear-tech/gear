use gear_core::message::Payload;
use gear_core_errors::{ReplyCode, SignalCode};

#[test]
fn check_message_codes_string_len() {
    for reply_code in enum_iterator::all::<ReplyCode>() {
        let _: Payload = reply_code.to_string().into_bytes().try_into().unwrap();
    }

    for signal_code in enum_iterator::all::<SignalCode>() {
        let _: Payload = signal_code.to_string().into_bytes().try_into().unwrap();
    }
}
