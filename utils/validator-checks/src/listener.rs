//! Block listener
use crate::{
    blocks_production::BlocksProduction,
    cmd::Opt,
    result::{Error, Result},
};
use futures_util::StreamExt;
use gp::api::{types::Blocks, Api};
use std::{result::Result as StdResult, time::Instant};
use subxt::ext::{
    sp_core::crypto::{PublicError, Ss58Codec},
    sp_runtime::AccountId32,
};

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
        self.api.finalized_blocks().await.map_err(Into::into)
    }

    /// Run validator checks.
    pub async fn check(&self) -> Result<()> {
        let validator_list = self.api.validators().await?;
        log::info!("Validators: {validator_list:#?}");

        let mut blocks_production = BlocksProduction::new(
            validator_list,
            (!self.opt.validators.is_empty()).then_some(
                self.opt
                    .validators
                    .iter()
                    .map(|acc| AccountId32::from_ss58check(&acc))
                    .collect::<StdResult<Vec<AccountId32>, PublicError>>()?,
            ),
        );

        let now = Instant::now();
        let mut blocks = self.listen().await?;
        while let Some(maybe_block) = blocks.next().await {
            if blocks_production.check(&maybe_block?) {
                break;
            }

            if now.elapsed().as_millis() > self.opt.timeout {
                log::error!(
                    "Some validators didn't produce blocks: {:#?}",
                    blocks_production.validators
                );

                return Err(Error::BlocksProduction);
            }
        }

        Ok(())
    }
}
