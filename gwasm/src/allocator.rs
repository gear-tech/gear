use core::alloc::{GlobalAlloc, Layout};

#[link(wasm_import_module = "gwasm-dlmalloc")]
extern "C" {
    fn alloc(size: usize, align: usize) -> *mut u8;
    fn dealloc(ptr: *mut u8, size: usize, align: usize);
    fn alloc_zeroed(size: usize, align: usize) -> *mut u8;
    fn realloc(ptr: *mut u8, align: usize, size: usize, new_size: usize) -> *mut u8;
}

/// A global allocator for the wasm32-unknown-unknown target that
/// uses the dlmalloc library.
pub struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        alloc(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        dealloc(ptr, layout.size(), layout.align())
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        alloc_zeroed(layout.size(), layout.align())
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        realloc(ptr, layout.align(), layout.size(), new_size)
    }
}
