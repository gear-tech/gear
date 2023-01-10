//! Shared traits.

use crate::{
    listener::Listener,
    result::Result,
    types::{Address, Block, Validators},
};
use async_trait::async_trait;

/// Author trait for getting the author of the current block.
pub trait Author {
    fn author(&self) -> Address;
}

impl Author for Block {
    fn author(&self) -> Address {
        todo!()
    }
}

/// Trait for creating a checker.
#[async_trait]
pub trait Checker: Sized {
    /// New checker.
    async fn new(listener: &Listener) -> Result<Self>;
}

/// Trait for doing various checks.
pub trait Check {
    fn name(&self) -> [u8; 4];

    /// Do the check.
    fn check(&self, validators: &mut Validators, block: &Block);
}
