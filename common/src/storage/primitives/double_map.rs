pub trait DoubleMapStorage {
    type Key1;
    type Key2;
    type Value;

    fn contains_key(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    fn elements_with(key1: &Self::Key1) -> usize;

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value>;

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value);

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> Option<R> {
        Self::mutate(key1, key2, |opt_val| opt_val.as_mut().map(f))
    }

    fn mutate_values<F: FnOnce(Self::Value) -> Self::Value>(f: F);

    fn remove(key1: Self::Key1, key2: Self::Key2);

    fn remove_all() -> Result<(), u8>;

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value>;
}
