use core::{
    ptr,
    task::{RawWaker, RawWakerVTable, Waker},
};

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

pub(crate) fn empty() -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(ptr::null(), &VTABLE)) }
}

unsafe fn clone_waker(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &VTABLE)
}
unsafe fn wake(_ptr: *const ()) {}
unsafe fn wake_by_ref(_ptr: *const ()) {}
unsafe fn drop_waker(_ptr: *const ()) {}
