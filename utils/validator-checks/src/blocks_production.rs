//! Utils for checking blocks production.
use gsdk::{config::GearConfig, ext::sp_runtime::AccountId32};
use parity_scale_codec::Decode;
use sp_consensus_babe::{digests::PreDigest as BabePreDigest, BABE_ENGINE_ID};
use subxt::{blocks, config::substrate::DigestItem, OnlineClient};

/// Gear block.
pub type Block = blocks::Block<GearConfig, OnlineClient<GearConfig>>;

/// Validator list
pub struct BlocksProduction {
    // All validators in the network.
    all_validators: Vec<AccountId32>,
    /// Validators to be checked.
    pub validators: Vec<AccountId32>,
}

impl BlocksProduction {
    /// New blocks production check from validator lists
    pub fn new(all_validators: Vec<AccountId32>, validators: Vec<AccountId32>) -> Self {
        Self {
            all_validators,
            validators,
        }
    }

    /// Check blocks production from block.
    ///
    /// Remove author of the block from validator list till the list is empty.
    ///
    /// Returns `true` if this validation is finished.
    pub fn check(&mut self, block: &Block) -> bool {
        if self.validators.is_empty() {
            return true;
        }

        let logs = &block.header().digest.logs;
        if let Some(DigestItem::PreRuntime(engine, bytes)) = logs.get(0) {
            if *engine == BABE_ENGINE_ID {
                if let Some(author) = BabePreDigest::decode(&mut bytes.as_ref())
                    .ok()
                    .and_then(|pre| self.all_validators.get(pre.authority_index() as usize))
                {
                    if let Ok(index) = self.validators.binary_search(author) {
                        log::info!(
                            "Validated {:?} for blocks production.",
                            self.validators.remove(index)
                        );
                    }
                }

                return self.validators.is_empty();
            }
        }

        false
    }
}
