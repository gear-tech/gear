pub trait MapStorage {
    type Key;
    type Value;

    fn contains_key(key: &Self::Key) -> bool;

    fn get(key: &Self::Key) -> Option<Self::Value>;

    fn insert(key: Self::Key, value: Self::Value);

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(key: Self::Key, f: F) -> Option<R> {
        Self::mutate(key, |opt_val| opt_val.as_mut().map(f))
    }

    fn mutate_values<F: FnOnce(Self::Value) -> Option<Self::Value>>(f: F);

    fn remove(key: Self::Key);

    fn remove_all() -> Result<(), u8>;

    fn take(key: Self::Key) -> Option<Self::Value>;
}
