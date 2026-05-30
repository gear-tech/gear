// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::signal::{ExceptionInfo, UserSignalHandler};
use cfg_if::cfg_if;
use nix::{
    libc::{c_void, siginfo_t},
    sys::{signal, signal::SigHandler},
};
use std::{io, sync::OnceLock};

/// Signal handler which has been set before lazy-pages initialization.
/// Currently use to support wasmtime signal handler.
/// Wasmtime protects memory around wasm memory and for stack limits.
/// It initializes its Unix trap handler lazily when an engine/store first
/// needs traps:
/// https://docs.wasmtime.dev/api/src/wasmtime/runtime/vm/traphandlers/signals.rs.html
/// Wasmtime's signal handler explicitly delegates unknown faults to the
/// previously installed process handler:
/// https://docs.wasmtime.dev/api/src/wasmtime/runtime/vm/sys/unix/signals.rs.html
/// So, if we receive signal from unknown memory we should try to use old (wasmtime) signal handler.
static OLD_SIG_HANDLER: OnceLock<SigHandler> = OnceLock::new();

cfg_if! {
    if #[cfg(all(target_os = "linux", target_arch = "x86_64"))] {
        unsafe fn ucontext_get_write(ucontext: *mut nix::libc::ucontext_t) -> Option<bool> {
            let error_reg = nix::libc::REG_ERR as usize;
            let error_code = unsafe { *ucontext }.uc_mcontext.gregs[error_reg];
            // Use the W/R bit from the page-fault error code.
            // See https://wiki.osdev.org/Exceptions#Page_Fault.
            Some(error_code & 0b10 == 0b10)
        }
    } else if #[cfg(all(target_os = "linux", target_arch = "aarch64"))] {
        unsafe fn ucontext_get_write(ucontext: *mut nix::libc::ucontext_t) -> Option<bool> {
            let esr = linux_aarch64::get_esr(&unsafe { &*ucontext }.uc_mcontext).expect("Failed to get ESR");
            // Use the WNR bit to determine if it was a write access.
            // See https://developer.arm.com/documentation/ddi0595/2021-03/AArch64-Registers/ESR-EL1--Exception-Syndrome-Register--EL1-?lang=en#fieldset_0-24_0_15-6_6
            let is_wnr = (esr & 0b100_0000) != 0;
            Some(is_wnr)
        }
    } else if #[cfg(all(target_os = "macos", target_arch = "x86_64"))] {
        unsafe fn ucontext_get_write(ucontext: *mut nix::libc::ucontext_t) -> Option<bool> {
            // See https://wiki.osdev.org/Exceptions
            const WRITE_BIT_MASK: u32 = 0b10;
            const TRAPNO: u16 = 0xe; // Page Fault

            let mcontext = unsafe { *ucontext }.uc_mcontext;
            let exception_state = unsafe { *mcontext }.__es;
            let trapno = exception_state.__trapno;
            let err = exception_state.__err;

            assert_eq!(trapno, TRAPNO);

            Some(err & WRITE_BIT_MASK == WRITE_BIT_MASK)
        }
    } else if #[cfg(all(target_os = "macos", target_arch = "aarch64"))] {
        unsafe fn ucontext_get_write(ucontext: *mut nix::libc::ucontext_t) -> Option<bool> {
            // See https://developer.arm.com/documentation/ddi0595/2021-03/AArch64-Registers/ESR-EL1--Exception-Syndrome-Register--EL1-
            const WNR_BIT_MASK: u32 = 0b100_0000; // Write not Read bit
            const EXCEPTION_CLASS_SHIFT: u32 = u32::BITS - 6;
            const EXCEPTION_CLASS: u32 = 0b10_0100; // Data Abort from a lower Exception Level

            let ucontext = unsafe { ucontext.as_mut() }?;
            let mcontext = ucontext.uc_mcontext;
            let exception_state = unsafe { *mcontext }.__es;
            let esr = exception_state.__esr;

            let exception_class = esr >> EXCEPTION_CLASS_SHIFT;
            assert_eq!(exception_class, EXCEPTION_CLASS);

            Some(esr & WNR_BIT_MASK == WNR_BIT_MASK)
        }
    } else {
        compile_error!("lazy-pages are not supported on your system. Disable `lazy-pages` feature");
    }
}

extern "C" fn handle_sigsegv<H>(sig: i32, info: *mut siginfo_t, ucontext: *mut c_void)
where
    H: UserSignalHandler,
{
    unsafe {
        let addr = (*info).si_addr();

        // Classify the fault before doing anything that is not
        // async-signal-safe. If the address is outside the WASM memory
        // lazy-pages currently manages on this thread, this is not a
        // lazy-pages page fault: the interrupted code may hold the
        // allocator or logger lock, so only async-signal-safe work is
        // allowed. Forward it without touching thread-locals or logging.
        if !crate::active_wasm_region_contains(addr as usize) {
            old_sig_handler(sig, info, ucontext);
            return;
        }

        // The fault is inside managed WASM memory: the thread was
        // executing WASM and holds no allocator/logger lock, so the
        // processing below (thread-local access, logging, page loading)
        // is safe in this context.
        let is_write = ucontext_get_write(ucontext as *mut _);
        let exc_info = ExceptionInfo {
            fault_addr: addr as *mut _,
            is_write,
        };

        if let Err(err) = H::handle(exc_info) {
            // The fault is inside managed WASM memory (classified above) but
            // `H::handle` could not service it — a lazy-pages invariant
            // violation, not a foreign fault. Panic: this thread was
            // executing WASM, so the panic runs safely and its backtrace
            // points at the bug.
            panic!("Signal handler failed: {err}");
        }
    }
}

use errno::Errno;

#[derive(Debug, Clone, Copy, derive_more::Display)]
enum ThreadInitError {
    #[display("Cannot get information about old signal stack: {_0}")]
    OldStack(Errno),
    #[display("Cannot mmap space for signal stack: {_0}")]
    Mmap(Errno),
    #[display("Cannot set new signal stack: {_0}")]
    SigAltStack(Errno),
}

fn init_for_thread_internal() -> Result<(), ThreadInitError> {
    use core::{mem, ptr};

    // Should be enough for lazy-pages signal handler.
    // Equal to libc::SIGSTKSZ on macos M1.
    const SIGNAL_STACK_SIZE: usize = 0x20000;

    enum StackInfo {
        UseOldStack,
        NewStack(*mut libc::c_void),
    }

    impl Drop for StackInfo {
        fn drop(&mut self) {
            if let StackInfo::NewStack(mmap_ptr) = self {
                unsafe {
                    // Deallocate the stack memory.
                    if libc::munmap(*mmap_ptr, SIGNAL_STACK_SIZE) != 0 {
                        log::error!(
                            "Cannot deallocate signal stack memory during the thread shutdown: {}",
                            errno::errno()
                        );
                    }
                }
            }
        }
    }

    unsafe fn init_sigstack() -> Result<StackInfo, ThreadInitError> {
        // Check whether old signal stack exist and suitable for lazy-pages signal handler.
        let mut old_stack = unsafe { mem::zeroed() };
        let res = unsafe { libc::sigaltstack(ptr::null(), &mut old_stack) };
        if res != 0 {
            return Err(ThreadInitError::OldStack(errno::errno()));
        }
        if old_stack.ss_flags & libc::SS_DISABLE == 0 && old_stack.ss_size >= SIGNAL_STACK_SIZE {
            return Ok(StackInfo::UseOldStack);
        }

        // Alloc memory for new signal stack.
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                SIGNAL_STACK_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(ThreadInitError::Mmap(errno::errno()));
        }

        // Mark allocated memory as new signal stack.
        let new_stack = libc::stack_t {
            ss_sp: ptr,
            ss_flags: 0,
            ss_size: SIGNAL_STACK_SIZE,
        };
        let res = unsafe { libc::sigaltstack(&new_stack, ptr::null_mut()) };
        if res != 0 {
            return Err(ThreadInitError::SigAltStack(errno::errno()));
        }

        log::debug!("Set new signal stack: ptr = {ptr:?}, size = {SIGNAL_STACK_SIZE:#x}");

        Ok(StackInfo::NewStack(ptr))
    }

    thread_local! {
        static TLS: Result<StackInfo, ThreadInitError> = unsafe { init_sigstack() };
    }

    TLS.with(|tls| tls.as_ref().map(|_| ()).map_err(|err| *err))
}

pub(crate) unsafe fn init_for_thread() -> Result<(), String> {
    init_for_thread_internal().map_err(|err| err.to_string())
}

pub(crate) unsafe fn setup_signal_handler<H>() -> io::Result<()>
where
    H: UserSignalHandler,
{
    let handler = signal::SigHandler::SigAction(handle_sigsegv::<H>);
    // SA_ONSTACK lets lazy-pages run on the alternate signal stack if the
    // fault is caused by stack overflow. SA_NODEFER keeps nested faults from
    // being masked, so an unhandled fault can still fall through to the
    // previously installed handler.
    let sig_action = signal::SigAction::new(
        handler,
        signal::SaFlags::SA_SIGINFO | signal::SaFlags::SA_ONSTACK | signal::SaFlags::SA_NODEFER,
        signal::SigSet::empty(),
    );

    let signal = if cfg!(target_os = "macos") {
        signal::SIGBUS
    } else {
        signal::SIGSEGV
    };

    let old_sigaction =
        unsafe { signal::sigaction(signal, &sig_action) }.map_err(io::Error::from)?;
    let handler = old_sigaction.handler();
    let _ = OLD_SIG_HANDLER
        .set(handler)
        .map(|_| log::trace!("Save old signal handler: {handler:?}"));

    Ok(())
}

unsafe fn old_sig_handler(sig: i32, info: *mut siginfo_t, ucontext: *mut c_void) {
    match OLD_SIG_HANDLER.get() {
        Some(SigHandler::Handler(func)) => func(sig),
        Some(SigHandler::SigAction(func)) => func(sig, info, ucontext),
        // No chainable previous handler exists: `SigDfl`/`SigIgn` carry no
        // function to call, and `None` means nothing was captured at install
        // time. Restore the default disposition so the re-executed faulting
        // instruction is terminated by the kernel's default action. The
        // disposition MUST be reset first: `SA_NODEFER` would otherwise
        // re-enter this handler on every re-fault, looping forever.
        Some(SigHandler::SigDfl | SigHandler::SigIgn) | None => {
            unsafe { libc::signal(sig, libc::SIG_DFL) };
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod linux_aarch64 {
    use std::{ptr, slice};

    const ESR_MAGIC: u32 = u32::from_be_bytes(*b"ESR\x01");

    #[repr(C, packed)]
    #[derive(Clone, Copy)]
    struct Header {
        magic: u32, // Magic number to identify the record type
        size: u32,  // Size of the record in bytes
    }

    /// Scan through the 4 KiB __reserved buffer looking for an `esr_context` record.
    /// Returns `Some(esr)` if we find a record whose magic == ESR_MAGIC, else `None`.
    /// See: https://github.com/torvalds/linux/blob/7f9039c524a351c684149ecf1b3c5145a0dff2fe/arch/arm64/include/uapi/asm/sigcontext.h#L40
    pub fn get_esr(mcontext: &nix::libc::mcontext_t) -> Option<usize> {
        // SAFETY: See `mcontext_t` definition:
        // ```C
        //  struct sigcontext {
        //      __u64 fault_address;
        //      /* AArch64 registers */
        //      __u64 regs[31];
        //      __u64 sp;
        //      __u64 pc;
        //      __u64 pstate;
        //      /* 4K reserved for FP/SIMD state and future expansion */
        //      __u8 __reserved[4096] __attribute__((__aligned__(16)));
        //  };
        // ```
        let reserved = unsafe {
            let reserved_addr_unaligned = ptr::addr_of!(mcontext.pstate).add(1);
            let reserved_addr =
                reserved_addr_unaligned.add(reserved_addr_unaligned.align_offset(16)) as *const u8;
            slice::from_raw_parts(reserved_addr, 4096)
        };

        let mut offset = 0usize;

        while offset + 8 <= reserved.len() {
            // Read header of the next context record:
            let Header { magic, size } =
                unsafe { (reserved.as_ptr().add(offset) as *const Header).read_unaligned() };
            let size = size as usize;

            // Sanity check: size must be at least 8 (header itself), and offset+size must not overflow 4096.
            if size < 8 || offset + size > reserved.len() {
                break;
            }

            if magic == ESR_MAGIC {
                // The first 8 bytes are (magic, size). The next 8 bytes are the u64 ESR value.
                if offset + 16 <= reserved.len() {
                    let esr_bytes = reserved[offset + 8..offset + 16]
                        .try_into()
                        .expect("cannot fail");
                    let esr = usize::from_ne_bytes(esr_bytes);
                    return Some(esr);
                } else {
                    // Not enough room for a full u64 after the header: treat as “not found”.
                    return None;
                }
            }

            // Skip to next record
            offset += size;
        }

        None
    }
}
