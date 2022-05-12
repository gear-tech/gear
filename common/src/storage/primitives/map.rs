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

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    fn remove(key: Self::Key);

    fn remove_all() -> Result<(), u32>;

    fn take(key: Self::Key) -> Option<Self::Value>;
}

#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_map {
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty) => {
        pub(crate) struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> MapStorage for $storage<T> {
            type Key = $key;
            type Value = $val;

            fn contains_key(key: &Self::Key) -> bool {
                $storage::<T>::contains_key(key)
            }

            fn get(key: &Self::Key) -> Option<Self::Value> {
                $storage::<T>::get(key)
            }

            fn insert(key: Self::Key, value: Self::Value) {
                $storage::<T>::insert(key, value)
            }

            fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R {
                $storage::<T>::mutate(key, f)
            }

            fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
                let f = |v| Some(f(v));
                $storage::<T>::translate_values(f)
            }

            fn remove(key: Self::Key) {
                $storage::<T>::remove(key)
            }

            fn remove_all() -> Result<(), u32> {
                match $storage::<T>::remove_all(None) {
                    sp_io::KillStorageResult::AllRemoved(..) => Ok(()),
                    sp_io::KillStorageResult::SomeRemaining(amount) => Err(amount),
                }
            }

            fn take(key: Self::Key) -> Option<Self::Value> {
                $storage::<T>::take(key)
            }
        }
    };
}
