/// Substrate StorageValue simplified interface.
pub trait StorageValue {
    type Value;

    /// Gets value from storage.
    fn get() -> Option<Self::Value>;

    /// Mutates value in storage with given function.
    fn mutate<R>(f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R;

    /// Removes and returns value from storage.
    fn remove() -> Option<Self::Value>;

    /// Sets given value in storage and returns previous one.
    fn set(value: Self::Value) -> Option<Self::Value>;
}

/// In-memory implementation of `crate::storage::StorageValue`.
pub trait TargetedStorageValue {
    type Value;

    /// Gets value from storage.
    fn get(&self) -> Option<Self::Value>;

    /// Mutates value in storage with given function.
    fn mutate<R>(&mut self, f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R;

    /// Removes and returns value from storage.
    fn remove(&mut self) -> Option<Self::Value>;

    /// Sets given value in storage and returns previous one.
    fn set(&mut self, value: Self::Value) -> Option<Self::Value>;
}
