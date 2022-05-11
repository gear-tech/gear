/// Callback trait for running some logic depent on conditions.
pub trait Callback<T, R = ()> {
    fn call(arg: &T) -> R;
}

/// Empty implementation for skipping callback.
impl<T> Callback<T> for () {
    fn call(_: &T) {}
}

pub trait EmptyCallback {
    fn call();
}

impl EmptyCallback for () {
    fn call() {}
}
