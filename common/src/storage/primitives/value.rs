pub trait ValueStorage {
    type Value;

    fn exists() -> bool;

    fn get() -> Option<Self::Value>;

    fn kill();

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(f: F) -> Option<R> {
        Self::mutate(|opt_val| opt_val.as_mut().map(f))
    }

    fn put(value: Self::Value);

    fn set(value: Self::Value) -> Option<Self::Value>;

    fn take() -> Option<Self::Value>;
}
