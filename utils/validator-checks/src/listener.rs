//! Block listener
use crate::{
    checks::BlockProduction,
    cmd::Opt,
    result::Result,
    traits::{Check, Checker},
};
use futures_util::StreamExt;
use gp::api::{types::FinalizedBlocks, Api};

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
    pub async fn listen(&self) -> Result<FinalizedBlocks> {
        self.api.finalized_blocks().await.map_err(Into::into)
    }

    /// Run validator checks.
    pub async fn check(&self) -> Result<()> {
        let mut checkers: Vec<Box<dyn Check>> = Default::default();
        if self.opt.produce_blocks {
            checkers.push(Box::new(BlockProduction::new(&self).await?));
        }

        let mut blocks = self.listen().await?;
        while let Some(maybe_block) = blocks.next().await {
            let block = maybe_block?;
            for checker in &checkers {
                checker.check(&block);
            }
        }

        Ok(())
    }
}
