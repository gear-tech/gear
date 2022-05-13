/// Substrate StorageMap simplified interface.
pub trait StorageMap {
    type Key;
    type Value;

    /// Checks if storage contains given key.
    fn contains(key: &Self::Key) -> bool;

    /// Gets value from storage by key.
    fn get(key: &Self::Key) -> Option<Self::Value>;

    /// Mutates value in storage with given function by key.
    fn mutate<R>(key: Self::Key, f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R;

    /// Removes and returns value from storage by key.
    fn remove(key: Self::Key) -> Option<Self::Value>;

    /// Sets given value in storage and returns previous one by key.
    fn set(key: Self::Key, value: Self::Value) -> Option<Self::Value>;
}

/// In-memory implementation of `crate::storage::StorageMap`.
pub trait TargetedStorageMap {
    type Key;
    type Value;

    /// Checks if storage contains given key.
    fn contains(&self, key: &Self::Key) -> bool;

    /// Gets value from storage by key.
    fn get(&self, key: &Self::Key) -> Option<Self::Value>;

    /// Mutates value in storage with given function by key.
    fn mutate<R>(&mut self, key: Self::Key, f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R;

    /// Removes and returns value from storage by key.
    fn remove(&mut self, key: Self::Key) -> Option<Self::Value>;

    /// Sets given value in storage and returns previous one by key.
    fn set(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value>;
}
