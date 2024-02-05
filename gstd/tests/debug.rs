use gstd::{debug, prelude::*};

static mut DEBUG_MSG: Vec<u8> = Vec::new();

mod sys {
    use super::*;

    #[no_mangle]
    unsafe extern "C" fn gr_debug(payload: *const u8, len: u32) {
        DEBUG_MSG.resize(len as _, 0);
        ptr::copy(payload, DEBUG_MSG.as_mut_ptr(), len as _);
    }
}

#[test]
#[allow(static_mut_ref)]
fn test_debug() {
    let value = 42;

    debug!("{value}");
    assert_eq!(unsafe { &DEBUG_MSG }, b"42");

    debug!("Formatted: value = {value}");
    assert_eq!(unsafe { &DEBUG_MSG }, b"Formatted: value = 42");

    debug!("String literal");
    assert_eq!(unsafe { &DEBUG_MSG }, b"String literal");

    crate::dbg!(value);
    assert_eq!(
        unsafe { &DEBUG_MSG },
        b"[gstd/tests/debug.rs:29:5] value = 42"
    );
}
