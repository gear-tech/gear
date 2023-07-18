//! Dummy library for exporting dlmalloc-rs
//!
//! This library contains dummy exports of all public methods of Dlmalloc of dlmalloc-rs
//! (including the `core::*` functions used by them).
//!
//! Public methods list:
//! - `malloc`
//! - `calloc`
//! - `free`
//! - `realloc`
#![no_std]

use dlmalloc::{Dlmalloc, GlobalDlmalloc};

#[global_allocator]
static ALLOCATOR: GlobalDlmalloc = GlobalDlmalloc;

/// `calloc` contains the usages of:
///  - `malloc`
///  - `free`
///  - `realloc`
///
/// so here we just export `calloc` and let the linker do the rest.
#[no_mangle]
pub unsafe extern "C" fn _calloc(size: usize, align: usize) -> *mut u8 {
    Dlmalloc::new().calloc(size, align)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
