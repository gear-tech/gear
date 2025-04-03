use anyhow::Result;
use ethexe_common::SimpleBlockData;
use ethexe_signer::Address;

use super::{producer::Producer, verifier::Verifier, ValidatorContext, ValidatorSubService};

pub struct Initial {
    ctx: ValidatorContext,
    state: State,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingForChainHead,
    WaitingForSyncedBlock(SimpleBlockData),
}

impl Initial {
    pub fn create(ctx: ValidatorContext) -> Result<Box<dyn ValidatorSubService>> {
        Ok(Box::new(Self {
            ctx,
            state: State::WaitingForChainHead,
        }))
    }

    pub fn create_with_chain_head(
        ctx: ValidatorContext,
        block: SimpleBlockData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        Ok(Box::new(Self {
            ctx,
            state: State::WaitingForSyncedBlock(block),
        }))
    }

    fn producer_for(&self, timestamp: u64, validators: &[Address]) -> Address {
        let slot = timestamp / self.ctx.slot_duration.as_secs();
        let index = Self::block_producer_index(validators.len(), slot);
        validators
            .get(index)
            .cloned()
            .unwrap_or_else(|| unreachable!("index must be valid"))
    }

    // TODO #4553: temporary implementation - next slot is the next validator in the list.
    const fn block_producer_index(validators_amount: usize, slot: u64) -> usize {
        (slot % validators_amount as u64) as usize
    }
}

impl ValidatorSubService for Initial {
    fn log(&self, s: String) -> String {
        format!("INITIAL in {state:?} - {s}", state = self.state)
    }

    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_synced_block(
        mut self: Box<Self>,
        data: ethexe_observer::BlockSyncedData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match &self.state {
            State::WaitingForChainHead => {
                let warning = self.log(format!("unexpected synced block: {:?}", data.block_hash));
                self.ctx.warning(warning);

                Ok(self)
            }
            State::WaitingForSyncedBlock(block) if block.hash == data.block_hash => {
                let producer = self.producer_for(block.header.timestamp, &data.validators);
                if self.ctx.pub_key.to_address() == producer {
                    Producer::create(self.ctx, block.clone(), data.validators)
                } else {
                    Verifier::create(self.ctx, block.clone(), producer)
                }
            }
            State::WaitingForSyncedBlock(block) => {
                let warning = self.log(format!(
                    "unexpected synced block: {:?}, must be {:?}",
                    block.hash, data.block_hash
                ));
                self.ctx.warning(warning);

                Ok(self)
            }
        }
    }
}
