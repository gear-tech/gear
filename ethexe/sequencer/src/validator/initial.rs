use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::{utils::BatchCommitmentValidationRequest, ControlEvent};
use anyhow::Result;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_signer::{sha3::digest::crypto_common::rand_core::block, Address, SignedData};
use futures::{stream::FusedStream, Stream};
use parity_scale_codec::Input;

use super::{
    producer::Producer, verifier::Verifier, InputEvent, ValidatorContext, ValidatorSubService,
};

pub struct Initial {
    ctx: ValidatorContext,
    state: State,
}

enum State {
    WaitingForChainHead,
    WaitingForSyncedBlock(SimpleBlockData),
}

impl Initial {
    pub fn new(ctx: ValidatorContext) -> Result<Box<dyn ValidatorSubService>> {
        Ok(Box::new(Self {
            ctx,
            state: State::WaitingForChainHead,
        }))
    }

    pub fn new_with_chain_head(
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
    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&mut self) -> &mut ValidatorContext {
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
                self.ctx.warning(format!(
                    "Received synced block while waiting for chain head: {:#x}",
                    data.block_hash
                ));

                Ok(self)
            }
            State::WaitingForSyncedBlock(block) if block.hash == data.block_hash => {
                let producer = self.producer_for(block.header.timestamp, &data.validators);
                if self.ctx.pub_key.to_address() == producer {
                    Producer::new(self.ctx, block.clone(), data.validators)
                } else {
                    Verifier::new(self.ctx, block.clone(), producer)
                }
            }
            State::WaitingForSyncedBlock(block) => {
                self.ctx.warning(format!(
                    "Synced block hash does not match the expected block hash: {:?} != {:?}",
                    block.hash, data.block_hash
                ));

                return Ok(self);
            }
        }
    }
}
