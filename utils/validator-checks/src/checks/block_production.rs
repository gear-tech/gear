//! Block production checks.

use crate::{
    listener::Listener,
    result::Result,
    traits::{Check, Checker},
    types::{Address, Block, Slot, Validators},
};
use async_trait::async_trait;
use parity_scale_codec::Decode;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_babe::{digests::PreDigest as BabePreDigest, BABE_ENGINE_ID};
use subxt::ext::sp_runtime::{generic::DigestItem, traits::Header};

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
        let validator_list = validators.validators();
        if let Some(DigestItem::PreRuntime(engine, bytes)) = logs.get(0) {
            if *engine == AURA_ENGINE_ID {
                Slot::decode(&mut bytes.as_ref())
                    .ok()
                    .map(|slot| slot.0 % validator_list.len() as u64)
            } else if *engine == BABE_ENGINE_ID {
                BabePreDigest::decode(&mut bytes.as_ref())
                    .ok()
                    .map(|pre| pre.authority_index() as u64)
            } else {
                None
            }
            .and_then(|author_index| validator_list.get(author_index as usize))
            .map(|author| {
                if validators.validated(&author, self.name()) {
                    log::info!(
                        "Validated {:?} for {}",
                        author,
                        String::from_utf8_lossy(&self.name())
                    );
                }
            });
        }
    }
}
