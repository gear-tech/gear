//! Block production checks.

use crate::{
    listener::Listener,
    result::Result,
    traits::{Check, Checker},
    types::{Address, Block, Validators},
};
use async_trait::async_trait;
use subxt::ext::sp_runtime::traits::Header;

pub struct BlockProduction(Vec<Address>);

#[async_trait]
impl Checker for BlockProduction {
    async fn new(listener: &Listener) -> Result<Self> {
        let validators = listener
            .opt
            .validators
            .clone()
            .into_iter()
            .map(Into::into)
            .collect();

        Ok(Self(validators))
    }
}

impl Check for BlockProduction {
    fn name(&self) -> [u8; 4] {
        *b"prod"
    }

    fn check(&self, validators: &mut Validators, block: &Block) {
        let logs = block.header().digest().logs();
        log::debug!("{:?}", logs);
    }
}
