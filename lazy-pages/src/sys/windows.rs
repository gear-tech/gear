use crate::sys::ExceptionInfo;
use std::io;
use winapi::{
    shared::ntdef::LONG,
    um::{
        errhandlingapi::SetUnhandledExceptionFilter, minwinbase::EXCEPTION_ACCESS_VIOLATION,
        winnt::EXCEPTION_POINTERS,
    },
    vc::excpt::{EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH},
};

unsafe extern "system" fn exception_handler(exception_info: *mut EXCEPTION_POINTERS) -> LONG {
    let exception_record = (*exception_info).ExceptionRecord;

    let is_access_violation = (*exception_record).ExceptionCode == EXCEPTION_ACCESS_VIOLATION;
    let num_params = (*exception_record).NumberParameters;
    if !is_access_violation || num_params != 2 {
        log::trace!(
            "Skip exception in handler: is access violation: {}, parameters: {}",
            is_access_violation,
            num_params
        );
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let addr = (*exception_record).ExceptionInformation[1];
    let info = ExceptionInfo {
        fault_addr: addr as *mut _,
    };

    super::memory_exception_handler(info).expect("Memory exception handler");

    EXCEPTION_CONTINUE_EXECUTION
}

pub unsafe fn setup_memory_exception_handler() -> io::Result<()> {
    SetUnhandledExceptionFilter(Some(exception_handler));
    Ok(())
}
