/// Substrate StorageValue simplified interface for counting.
pub trait StorageCounter {
    type Value;

    /// Gets value from storage.
    fn get() -> Self::Value;

    /// Increase the value in storage.
    fn increase();

    /// Decrease the value in storage.
    fn decrease();

    /// Clear the value in storage.
    fn clear();
}

/// In-memory implementation of `crate::storage::StorageCounter`.
pub trait TargetedStorageCounter {
    type Value;

    /// Gets value from storage.
    fn get(&self) -> Self::Value;

    /// Increase the value in storage.
    fn increase(&mut self);

    /// Decrease the value in storage.
    fn decrease(&mut self);

    /// Clear the value in storage.
    fn clear(&mut self);
}
