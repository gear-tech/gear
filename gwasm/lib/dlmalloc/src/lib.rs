//! Dummy library for exporting dlmalloc-rs as wasm module.
#![no_std]
// #![feature(wasm_import_memory)]
// #![wasm_import_memory]

use dlmalloc::Dlmalloc;

static mut DLMALLOC: Dlmalloc = Dlmalloc::new();

/// Allocate memory as described by the given `layout`.
#[no_mangle]
pub unsafe extern "C" fn alloc(size: usize, align: usize) -> *mut u8 {
    DLMALLOC.malloc(size, align)
}

/// Deallocate the block of memory at the given `ptr` pointer with the given `layout`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, size: usize, align: usize) {
    DLMALLOC.free(ptr, size, align)
}

/// Behaves like `alloc`, but also ensures that the contents
/// are set to zero before being returned.
#[no_mangle]
pub unsafe extern "C" fn alloc_zeroed(size: usize, align: usize) -> *mut u8 {
    DLMALLOC.calloc(size, align)
}

/// Shrink or grow a block of memory to the given `new_size` in bytes.
/// The block is described by the given `ptr` pointer and `layout`.
#[no_mangle]
pub unsafe extern "C" fn realloc(
    ptr: *mut u8,
    size: usize,
    align: usize,
    new_size: usize,
) -> *mut u8 {
    DLMALLOC.realloc(ptr, size, align, new_size)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
