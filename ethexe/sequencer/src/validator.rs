use crate::{
    bp::{ControlError, ControlEvent, ControlService},
    coordinator::Coordinator,
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
    pub_key: PublicKey,
    signer: Signer,
    db: Database,
    ethereum: Ethereum,
    state: State,
    output: VecDeque<ControlEvent>,
}

#[derive(Default)]
enum State {
    #[default]
    Initial,
    NewChainHeadReceived(ChainHeadReceived),
    Producer(Producer),
    Verifier(Verifier),
    Coordinator(Coordinator),
    Participant(Participant),
    Submitting(BoxFuture<'static, anyhow::Result<H256>>),
}

pub struct ChainHeadReceived {
    block: SimpleBlockData,
    producer_blocks: Vec<SignedData<ProducerBlock>>,
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

        let block = state.block.clone();

        if data.block_hash != block.hash {
            return Err(ControlError::Warning(anyhow!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash,
                block.hash
            )));
        }

        let slot = block.header.timestamp / self.slot_duration.as_secs();
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
                let batch = producer.receive_computed_block(computed_block)?;

                let State::Producer(producer) = mem::take(&mut self.state) else {
                    unreachable!("state must be Producer");
                };

                if let Some(batch) = batch {
                    let (coordinator, events) = Coordinator::new(batch, producer, self.threshold)?;

                    self.output.extend(events);
                    self.state = State::Coordinator(coordinator);
                }
            }
            State::Verifier(verifier) => {
                if verifier.receive_computed_block(computed_block)? {
                    let State::Verifier(verifier) = mem::take(&mut self.state) else {
                        unreachable!("state must be Verifier");
                    };

                    // TODO: it's a bug fix me
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
            State::Initial | State::Producer(_) | State::NewChainHeadReceived(_) => Err(
                ControlError::Warning(anyhow!("Received validation reply in unexpected state")),
            )?,
        }

        Ok(())
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
