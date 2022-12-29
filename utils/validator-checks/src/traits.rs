//! Shared traits.

use crate::types::Address;

/// Author trait for getting author of block.
pub trait Author {
    fn author(&self) -> Address;
}

/// Trait for various checks
pub trait Check<Block> {
    fn check(block: &Block);
}
