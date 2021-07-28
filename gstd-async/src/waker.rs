use alloc::sync::Arc;
use core::task::{RawWaker, RawWakerVTable, Waker};

struct WakerFn<F>(F);

impl<F: Fn() + Send + Sync + 'static> WakerFn<F> {
    const VTABLE: RawWakerVTable =
        RawWakerVTable::new(Self::clone, Self::wake, Self::wake_by_ref, Self::drop);

    unsafe fn clone(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &Self::VTABLE)
    }

    unsafe fn wake(ptr: *const ()) {
        let f = Arc::from_raw(ptr as *const F);
        (f)();
    }

    unsafe fn wake_by_ref(ptr: *const ()) {
        Self::wake(ptr);
    }

    unsafe fn drop(ptr: *const ()) {
        drop(Arc::from_raw(ptr as *const F));
    }
}

pub(crate) fn from_fn<F: Fn() + Send + Sync + 'static>(f: F) -> Waker {
    let raw = Arc::into_raw(Arc::new(f)) as *const ();
    unsafe { Waker::from_raw(RawWaker::new(raw, &WakerFn::<F>::VTABLE)) }
}
