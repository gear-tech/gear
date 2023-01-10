//! Block listener
use crate::{
    checks::BlockProduction,
    cmd::Opt,
    result::Result,
    traits::{Check, Checker},
};
use futures_util::StreamExt;
use gp::api::{types::Blocks, Api};

/// Entry of this program, block listener.
pub struct Listener {
    /// Gear API.
    pub api: Api,
    /// Listener configuration.
    pub opt: Opt,
}

impl Listener {
    /// Create new block listener.
    pub async fn new(opt: Opt) -> Result<Self> {
        Ok(Self {
            api: Api::new(opt.endpoint.as_deref()).await?,
            opt,
        })
    }

    /// Listen to finalized blocks.
    pub async fn listen(&self) -> Result<Blocks> {
        self.api.blocks().await.map_err(Into::into)
    }

    /// Run validator checks.
    pub async fn check(&self) -> Result<()> {
        let mut checkers: Vec<Box<dyn Check>> = Default::default();
        let mut validators = self.api.validators().await?.into();

        if self.opt.block_production {
            checkers.push(Box::new(BlockProduction::new(&self).await?));
        }

        let mut blocks = self.listen().await?;
        while let Some(maybe_block) = blocks.next().await {
            let block = maybe_block?;
            for checker in &checkers {
                checker.check(&mut validators, &block);
            }

            if validators.validate_all(
                &checkers
                    .iter()
                    .map(|checker| checker.name())
                    .collect::<Vec<[u8; 4]>>(),
            ) {
                break;
            }
        }

        Ok(())
    }
}
