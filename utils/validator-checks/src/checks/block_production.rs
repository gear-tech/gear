//! Block production checks.

use crate::{
    listener::Listener,
    result::Result,
    traits::{Check, Checker},
    types::{Address, Block},
};
use async_trait::async_trait;

pub struct BlockProduction(Vec<Address>);

#[async_trait]
impl Checker for BlockProduction {
    async fn new(_listener: &Listener) -> Result<Self> {
        todo!()
    }
}

impl Check for BlockProduction {
    fn check(&self, _block: &Block) {
        todo!()
    }
}
