use crate::{
    bp::{ControlError, ControlEvent, ControlService},
    participant::Participant,
    producer::Producer,
    utils::{BatchCommitmentValidationRequest, MultisignedCommitmentsBatch},
    verifier::Verifier,
};
use anyhow::anyhow;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_db::Database;
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{PublicKey, SignedData, Signer};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

pub struct ValidatorService {
    slot_duration: u64,
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    state: State,
    output: VecDeque<ControlEvent>,
}

enum State {
    Initial,
    NewChainHeadReceived(ChainHeadReceived),
    Producer(Producer),
    Verifier(Verifier),
    Coordinator(Coordinator),
    Participant(Participant),
}

pub struct ChainHeadReceived {
    block: SimpleBlockData,
    producer_blocks: Vec<SignedData<ProducerBlock>>,
}

impl ValidatorService {
    pub fn new(pub_key: PublicKey, signer: Signer, db: Database, slot_duration: u64) -> Self {
        Self {
            slot_duration,
            pub_key,
            signer,
            db,
            state: State::Initial,
            output: VecDeque::new(),
        }
    }

    // TODO #4553: temporary implementation - next slot is the next validator in the list.
    const fn block_producer_index(validators_amount: usize, slot: u64) -> usize {
        (slot % validators_amount as u64) as usize
    }
}

impl ControlService for ValidatorService {
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) {
        // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.
        self.state = State::NewChainHeadReceived(ChainHeadReceived {
            block,
            producer_blocks: Vec::new(),
        });
    }

    /// Returns whether synced block is previously received chain head.
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<(), ControlError> {
        let state = match &mut self.state {
            State::NewChainHeadReceived(state) => state,
            _ => {
                return Err(ControlError::Warning(anyhow!(
                    "Received unexpected synced block {}",
                    data.block_hash
                )))
            }
        };

        // TODO (gsobol): how to remove this clone?
        let block = state.block.clone();

        if data.block_hash != block.hash {
            log::warn!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash,
                block.hash
            );
            return Ok(());
        }

        let slot = block.header.timestamp / self.slot_duration;
        let index = Self::block_producer_index(data.validators.len(), slot);
        let producer = data
            .validators
            .get(index)
            .cloned()
            .unwrap_or_else(|| unreachable!("index must be valid"));

        if self.pub_key.to_address() == producer {
            let (producer, events) = Producer::new(
                self.pub_key,
                self.signer.clone(),
                self.db.clone(),
                data.validators,
                block,
            )?;
            self.state = State::Producer(producer);
            self.output.extend(events);
        } else {
            let (verifier, events) =
                Verifier::new(block, producer, mem::take(&mut state.producer_blocks))?;
            self.output.extend(events);
            self.state = State::Verifier(verifier);
        }

        Ok(())
    }

    fn receive_block_from_producer(
        &mut self,
        signed: SignedData<ProducerBlock>,
    ) -> Result<(), ControlError> {
        match &mut self.state {
            State::Initial => {
                // +_+_+: collect producer blocks in this state.
                Err(ControlError::EventSkipped)?;
            }
            State::NewChainHeadReceived(state) => {
                if signed.data().block_hash != state.block.hash {
                    Err(ControlError::Warning(anyhow!(
                        "Received block {} is different from the expected block hash {}",
                        signed.data().block_hash,
                        state.block.hash
                    )))?;
                }

                // Wait for the block to be synced.
                state.producer_blocks.push(signed);
            }
            State::Verifier(verifier) => {
                let events = verifier.receive_block_from_producer(signed)?;
                self.output.extend(events);
            }
            State::Producer(_) => Err(ControlError::Warning(anyhow!(
                "Received producer block in producer mode"
            )))?,
            _ => Err(ControlError::Warning(anyhow!(
                "Received producer block in unexpected state"
            )))?,
        }

        Ok(())
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<(), ControlError> {
        match &mut self.state {
            State::Producer(producer) => {
                if let Some(batch) = producer.receive_computed_block(computed_block)? {
                    let (multisigned_batch, validation_request) =
                        MultisignedCommitmentsBatch::new_with_validation_request(
                            batch,
                            &self.signer,
                            self.pub_key,
                        )?;
                    self.output
                        .push_back(ControlEvent::PublishValidationRequest(validation_request));
                    // +_+_+
                    self.state = State::Coordinator(Coordinator {});
                } else {
                    // Empty block - nothing to do until next chain head.
                    self.state = State::Initial;
                }
            }
            State::Verifier(verifier) => {
                if verifier.receive_computed_block(computed_block)? {
                    let (participant, events) = Participant::new(
                        self.pub_key,
                        self.db.clone(),
                        self.signer.clone(),
                        verifier,
                    )?;

                    self.output.extend(events);

                    match participant {
                        Some(participant) => self.state = State::Participant(participant),
                        None => self.state = State::Initial,
                    }
                }
            }
            _ => Err(ControlError::Warning(anyhow!(
                "Received computed block in unexpected state"
            )))?,
        }

        Ok(())
    }

    fn receive_validation_request(
        &mut self,
        signed_request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<(), ControlError> {
        match &mut self.state {
            State::Participant(participant) => {
                let events = participant.receive_validation_request(signed_request)?;
                self.output.extend(events);
                self.state = State::Initial;
            }
            State::Verifier(verifier) => {
                verifier.receive_validation_request(signed_request)?;
            }
            State::Coordinator(_) => Err(ControlError::Warning(anyhow!(
                "Received validation request in coordinator mode"
            )))?,
            _ => Err(ControlError::Warning(anyhow!(
                "Received validation request in unexpected state"
            )))?,
        }

        Ok(())
    }

    fn is_block_producer(&self) -> anyhow::Result<bool> {
        // +_+_+
        todo!()
    }
}

impl Stream for ValidatorService {
    type Item = ControlEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(event))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for ValidatorService {
    fn is_terminated(&self) -> bool {
        false
    }
}

pub struct Coordinator {}
