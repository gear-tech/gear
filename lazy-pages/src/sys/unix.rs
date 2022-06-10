use crate::sys::ExceptionInfo;
use nix::{
    libc::{c_void, siginfo_t},
    sys::signal,
};
use std::io::{self};

extern "C" fn handle_sigsegv(_x: i32, info: *mut siginfo_t, _z: *mut c_void) {
    unsafe {
        let addr = (*info).si_addr();
        let info = ExceptionInfo {
            fault_addr: addr as *mut _,
        };

        super::memory_exception_handler(info).expect("Memory exception handler");
    }
}

pub unsafe fn setup_memory_exception_handler() -> io::Result<()> {
    let handler = signal::SigHandler::SigAction(handle_sigsegv);
    let sig_action = signal::SigAction::new(
        handler,
        signal::SaFlags::SA_SIGINFO,
        signal::SigSet::empty(),
    );

    let signal = if cfg!(target_os = "macos") {
        signal::SIGBUS
    } else {
        signal::SIGSEGV
    };

    let res = signal::sigaction(signal, &sig_action);
    if let Err(err_no) = res {
        return Err(io::Error::from_raw_os_error(err_no as i32));
    }

    Ok(())
}
