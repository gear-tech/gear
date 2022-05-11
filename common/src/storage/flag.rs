/// Substrate StorageValue simplified interface for boolean flag.
pub trait StorageFlag {
    /// Allow logic.
    fn allow();

    /// Deny logic.
    fn deny();

    /// Get permission flag.
    fn allowed() -> bool;

    /// Get denied permission flag.
    fn denied() -> bool {
        !Self::allowed()
    }
}

/// In-memory implementation of `crate::storage::StorageFlag`.
pub trait TargetedStorageFlag {
    /// Allow logic.
    fn allow(&mut self);

    /// Deny logic.
    fn deny(&mut self);

    /// Get permission flag.
    fn allowed(&self) -> bool;

    /// Get denied permission flag.
    fn denied(&self) -> bool {
        !self.allowed()
    }
}
