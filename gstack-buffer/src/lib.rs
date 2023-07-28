#![feature(c_unwind)]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, MaybeUninit},
    slice,
};

const MAX_BUFFER_SIZE: usize = 64 * 1024;

type Callback = unsafe extern "C-unwind" fn(ptr: *mut MaybeUninit<u8>, data: *mut c_void);

#[cfg(any(
    feature = "compile-alloca",
    all(not(feature = "compile-alloca"), target_arch = "wasm32")
))]
extern "C" {
    fn c_with_alloca(size: usize, callback: Callback, data: *mut c_void);
}

#[cfg(all(not(feature = "compile-alloca"), not(target_arch = "wasm32")))]
unsafe extern "C" fn c_with_alloca(_size: usize, callback: Callback, data: *mut c_void) {
    let mut buffer = MaybeUninit::<[MaybeUninit<u8>; MAX_BUFFER_SIZE]>::uninit().assume_init();
    callback(buffer.as_mut_ptr(), data);
}

#[inline(always)]
fn get_trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(_closure: &F) -> Callback {
    trampoline::<F>
}

unsafe extern "C-unwind" fn trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(
    ptr: *mut MaybeUninit<u8>,
    data: *mut c_void,
) {
    let f = ManuallyDrop::take(&mut *(data as *mut ManuallyDrop<F>));
    f(ptr);
}

fn with_alloca<T>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> T) -> T {
    let mut ret = MaybeUninit::uninit();

    let closure = |ptr| {
        let slice = unsafe { slice::from_raw_parts_mut(ptr, size) };
        ret.write(f(slice));
    };

    let trampoline = get_trampoline(&closure);
    let mut closure_data = ManuallyDrop::new(closure);

    unsafe {
        c_with_alloca(size, trampoline, &mut closure_data as *mut _ as *mut c_void);
        ret.assume_init()
    }
}

pub fn with_byte_buffer<T>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> T) -> T {
    if size <= MAX_BUFFER_SIZE {
        with_alloca(size, f)
    } else {
        f(Vec::with_capacity(size).spare_capacity_mut())
    }
}
