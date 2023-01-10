//! Block listener
use crate::{
    checks::BlockProduction,
    cmd::Opt,
    result::Result,
    traits::{Check, Checker},
};
use futures_util::StreamExt;
use gp::api::{types::Blocks, Api};
use std::time::Instant;

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
        let mut checkers: Vec<Box<dyn Check>> = Default::default();
        let validator_list = self.api.validators().await?;
        log::info!("Validators: {validator_list:#?}");

        if self.opt.block_production {
            checkers.push(Box::new(BlockProduction::new(&self).await?));
        }

        let now = Instant::now();
        let checkers_len = checkers.len();
        let mut validators = validator_list.into();
        let mut blocks = self.listen().await?;
        let all_checks = checkers
            .iter()
            .map(|checker| checker.name())
            .collect::<Vec<[u8; 4]>>();
        while let Some(maybe_block) = blocks.next().await {
            let block = maybe_block?;
            for checker in &checkers {
                checker.check(&mut validators, &block);
            }

            if now.elapsed().as_millis() > self.opt.timeout {
                log::error!(
                    "Some checks didn't pass: {:#?}",
                    validators
                        .unvalidated(&all_checks)
                        .iter()
                        .map(|(name, v)| { (String::from_utf8_lossy(name).to_string(), v) })
                        .collect::<Vec<(String, &Vec<_>)>>()
                );

                std::process::exit(1);
            }

            if validators.validate_all(&all_checks).len() == checkers_len {
                break;
            }
        }

        Ok(())
    }
}
