//! Block listener
use crate::{
    blocks_production::BlocksProduction,
    cmd::Opt,
    result::{Error, Result},
};
use futures_util::StreamExt;
use gsdk::{
    ext::{
        sp_core::crypto::{PublicError, Ss58Codec},
        sp_runtime::AccountId32,
    },
    types::Blocks,
    Api,
};
use std::{result::Result as StdResult, time::Instant};

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
    pub async fn listen_finalized(&self) -> Result<Blocks> {
        self.api.finalized_blocks().await.map_err(Into::into)
    }

    /// Run validator checks.
    pub async fn check(&self) -> Result<()> {
        let all_validators = self.api.validators().await?;
        let validators_to_be_checked = if !self.opt.validators.is_empty() {
            self.opt
                .validators
                .iter()
                .map(|acc| AccountId32::from_ss58check(acc))
                .collect::<StdResult<Vec<AccountId32>, PublicError>>()?
        } else {
            all_validators.clone()
        };

        log::info!("All validators: {all_validators:#?}");
        log::info!("Validators to be checked: {validators_to_be_checked:#?}");

        let mut blocks_production = BlocksProduction::new(all_validators, validators_to_be_checked);

        let now = Instant::now();
        let mut blocks = self.listen_finalized().await?;
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
