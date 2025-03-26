use crate::{
    bp::{ControlError, ControlEvent, ControlService},
    coordinator::Coordinator,
    initial::{Initial, Unformed},
    participant::Participant,
    producer::Producer,
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        MultisignedBatchCommitment,
    },
    verifier::Verifier,
};
use anyhow::anyhow;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_db::Database;
use ethexe_ethereum::{router::Router, Ethereum};
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{Address, PublicKey, SignedData, Signer};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    mem,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub struct ValidatorService {
    slot_duration: Duration,
    threshold: u64,
    router_address: Address,
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    ethereum: Ethereum,
    state: State,
    output: VecDeque<ControlEvent>,
}

enum State {
    Initial(Initial),
    Unformed(Unformed),
    Producer(Producer),
    Verifier(Verifier),
    Coordinator(Coordinator),
    Participant(Participant),
    Submitting(BoxFuture<'static, anyhow::Result<H256>>),
}

impl Default for State {
    fn default() -> Self {
        Self::Initial(Initial::default())
    }
}

pub struct ValidatorConfig {
    pub ethereum_rpc: String,
    pub pub_key: PublicKey,
    pub router_address: Address,
    pub threshold: u64,
    pub slot_duration: Duration,
}

impl ValidatorService {
    pub async fn new(
        signer: Signer,
        db: Database,
        config: ValidatorConfig,
    ) -> anyhow::Result<Self> {
        let ethereum = Ethereum::new(
            &config.ethereum_rpc,
            config.router_address,
            signer.clone(),
            config.pub_key.to_address(),
        )
        .await?;

        Ok(Self {
            slot_duration: config.slot_duration,
            threshold: config.threshold,
            router_address: config.router_address,
            pub_key: config.pub_key,
            signer,
            db,
            state: State::default(),
            output: VecDeque::new(),
            ethereum,
        })
    }

    // TODO #4553: temporary implementation - next slot is the next validator in the list.
    const fn block_producer_index(validators_amount: usize, slot: u64) -> usize {
        (slot % validators_amount as u64) as usize
    }

    fn producer_for(&self, timestamp: u64, validators: &[Address]) -> Address {
        let slot = timestamp / self.slot_duration.as_secs();
        let index = Self::block_producer_index(validators.len(), slot);
        validators
            .get(index)
            .cloned()
            .unwrap_or_else(|| unreachable!("index must be valid"))
    }
}

impl ControlService for ValidatorService {
    // TODO #4555: block producer could be calculated right here, using propagation from previous blocks.
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) {
        let state = mem::take(&mut self.state);
        self.state = State::Unformed(match state {
            State::Initial(initial) => initial.into_unformed(block),
            State::Unformed(unformed) => unformed.with_new_chain_head(block),
            _ => Unformed::new(block),
        });
    }

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<(), ControlError> {
        let State::Unformed(state) = &mut self.state else {
            return Err(ControlError::Warning(anyhow!(
                "Received synced block {} in unexpected state",
                data.block_hash
            )));
        };

        if data.block_hash != state.block().hash {
            return Err(ControlError::Warning(anyhow!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash,
                state.block().hash
            )));
        }

        let State::Unformed(state) = mem::take(&mut self.state) else {
            unreachable!("state must be Unformed");
        };
        let (block, blocks, requests) = state.into_parts();

        let producer = self.producer_for(block.header.timestamp, &data.validators);

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
            let (verifier, events) = Verifier::new(block, producer, blocks, requests)?;
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
            State::Initial(initial) => {
                initial.receive_block_from_producer(signed);
            }
            State::Unformed(unformed) => {
                unformed.receive_block_from_producer(signed);
            }
            State::Verifier(verifier) => {
                let events = verifier.receive_block_from_producer(signed)?;
                self.output.extend(events);
            }
            State::Producer(_) | State::Coordinator(_) | State::Submitting(_) => Err(
                ControlError::Warning(anyhow!("Received producer block, but I'm producer")),
            )?,
            State::Participant(_) => Err(ControlError::Warning(anyhow!(
                "Received producer block in unexpected state"
            )))?,
        }

        Ok(())
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<(), ControlError> {
        match &mut self.state {
            State::Producer(producer) => {
                let batch = producer.receive_computed_block(computed_block)?;

                let State::Producer(producer) = mem::take(&mut self.state) else {
                    unreachable!("state must be Producer");
                };

                if let Some(batch) = batch {
                    let (validators, _block) = producer.into_parts();

                    let (coordinator, events) = Coordinator::new(
                        self.pub_key,
                        validators,
                        self.threshold,
                        self.router_address,
                        batch,
                        self.signer.clone(),
                    )?;

                    self.output.extend(events);
                    self.state = State::Coordinator(coordinator);
                }
            }
            State::Verifier(verifier) => {
                if verifier.receive_computed_block(computed_block)? {
                    let State::Verifier(verifier) = mem::take(&mut self.state) else {
                        unreachable!("state must be Verifier");
                    };

                    let (producer, block, request) = verifier.into_parts();

                    let participant = Participant::new(
                        self.pub_key,
                        self.router_address,
                        producer,
                        block,
                        self.db.clone(),
                        self.signer.clone(),
                    );

                    self.state = State::Participant(participant);

                    let State::Participant(participant) = &mut self.state else {
                        unreachable!("state must be Participant");
                    };

                    if let Some(request) = request {
                        let events = participant.receive_validation_request_unsigned(request)?;
                        self.output.extend(events);
                        self.state = State::default();
                    }
                }
            }
            State::Initial(_)
            | State::Unformed(_)
            | State::Coordinator(_)
            | State::Participant(_)
            | State::Submitting(_) => Err(ControlError::Warning(anyhow!(
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
            State::Initial(initial) => {
                initial.receive_validation_request(signed_request);
            }
            State::Unformed(unformed) => {
                unformed.receive_validation_request(signed_request);
            }
            State::Participant(participant) => {
                let events = participant.receive_validation_request(signed_request)?;
                self.output.extend(events);
                self.state = State::default();
            }
            State::Verifier(verifier) => {
                verifier.receive_validation_request(signed_request)?;
            }
            State::Coordinator(_) | State::Producer(_) | State::Submitting(_) => Err(
                ControlError::Warning(anyhow!("Received validation request, but I'm producer")),
            )?,
        }

        Ok(())
    }

    fn receive_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<(), ControlError> {
        match &mut self.state {
            State::Coordinator(coordinator) => {
                if !coordinator.receive_validation_reply(reply)? {
                    return Ok(());
                }

                let State::Coordinator(coordinator) = mem::take(&mut self.state) else {
                    unreachable!("state must be Coordinator");
                };

                let batch = coordinator.into_multisigned_batch_commitment();

                self.state = State::Submitting(
                    submit_batch_commitment(self.ethereum.router(), batch).boxed(),
                );
            }
            State::Verifier(_) | State::Submitting(_) => Err(ControlError::EventSkipped)?,
            State::Participant(_) => Err(ControlError::Warning(anyhow!(
                "Received validation reply in participant mode"
            )))?,
            State::Initial(_) | State::Unformed(_) | State::Producer(_) => Err(
                ControlError::Warning(anyhow!("Received validation reply in unexpected state")),
            )?,
        }

        Ok(())
    }

    fn is_block_producer(&self) -> anyhow::Result<bool> {
        match &self.state {
            State::Producer(_) | State::Coordinator(_) | State::Submitting(_) => Ok(true),
            State::Verifier(_) | State::Participant(_) => Ok(false),
            State::Initial(_) | State::Unformed(_) => Err(anyhow!("Is not known yet")),
        }
    }
}

impl Stream for ValidatorService {
    type Item = ControlEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let State::Submitting(future) = &mut self.state {
            match future.poll_unpin(_cx) {
                Poll::Ready(result) => {
                    self.output
                        .push_back(ControlEvent::SubmissionResult(result.map_err(|e| {
                            ControlError::Warning(anyhow!("Failed to submit batch commitment: {e}"))
                        })));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

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

async fn submit_batch_commitment(
    router: Router,
    batch: MultisignedBatchCommitment,
) -> anyhow::Result<H256> {
    let (commitment, signatures) = batch.into_parts();
    let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

    log::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

    router.commit_batch(commitment, signatures).await
}
