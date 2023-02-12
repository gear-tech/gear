use gear_core::buffer::RuntimeBuffer;
use gear_core_errors::{SimpleReplyError, SimpleSignalError};

#[test]
fn check_simple_errors_string_len() {
    for err in enum_iterator::all::<SimpleReplyError>() {
        let _: RuntimeBuffer = err.to_string().into_bytes().try_into().unwrap();
    }

    for err in enum_iterator::all::<SimpleSignalError>() {
        let _: RuntimeBuffer = err.to_string().into_bytes().try_into().unwrap();
    }
}
