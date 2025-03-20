// +_+_+ doc

use std::{
    collections::VecDeque,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    agro::{AggregatedCommitments, SignedCommitmentsBatch},
    producer::{Producer, ProducerState},
};
use anyhow::anyhow;
use ethexe_common::{
    gear::{BlockCommitment, CodeCommitment},
    SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{
    sha3::{digest::Update, Keccak256},
    Address, Digest, PublicKey, Signature, Signer, ToDigest,
};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use parity_scale_codec::Encode;

pub trait ControlService: Stream<Item = ControlEvent> + FusedStream {
    fn receive_new_chain_head(&mut self, block: SimpleBlockData);
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<(), ControlError>;
    fn receive_block_from_producer(
        &mut self,
        block: SignedProducerBlock,
    ) -> Result<(), ControlError>;
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<(), ControlError>;
    fn is_block_producer(&self) -> anyhow::Result<bool>;
}

#[derive(Clone, Debug)]
pub struct ProducerBlock {
    pub block_hash: H256,
    pub gas_allowance: Option<u64>,
    // +_+_+ consider. Maybe need to share off-chain transactions data.
    pub off_chain_transactions: Vec<H256>,
}

impl ToDigest for ProducerBlock {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.block_hash.as_bytes());
        hasher.update(self.gas_allowance.encode().as_slice());
        hasher.update(self.off_chain_transactions.encode().as_slice());
    }
}

#[derive(Debug, derive_more::From)]
pub enum ControlError {
    #[from]
    Common(anyhow::Error),
    Warning(anyhow::Error),
    EventSkipped,
}

pub enum ControlEvent {
    ComputeBlock(H256),
    ComputeProducerBlock(ProducerBlock),
    PublishProducerBlock(SignedProducerBlock),
    RequestValidation(SignedCommitmentsBatch),
    SendValidationRequest,
}

pub struct SignedProducerBlock {
    pub block: ProducerBlock,
    pub ecdsa_signature: Signature,
}

pub struct SimpleConnectService {
    block: Option<SimpleBlockData>,
    output: VecDeque<ControlEvent>,
}

impl SimpleConnectService {
    pub fn new() -> Self {
        Self {
            block: None,
            output: VecDeque::new(),
        }
    }
}

impl ControlService for SimpleConnectService {
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) {
        self.block = Some(block);
    }

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<(), ControlError> {
        let Some(block) = self.block.as_ref() else {
            return Err(ControlError::Common(anyhow!(
                "Received synced block {}, but no chain-head was received yet",
                data.block_hash
            )));
        };

        if block.hash != data.block_hash {
            return Err(ControlError::Warning(anyhow!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash,
                block.hash
            )));
        }

        self.output
            .push_back(ControlEvent::ComputeBlock(block.header.parent_hash));

        Ok(())
    }

    fn receive_block_from_producer(
        &mut self,
        _block_hash: SignedProducerBlock,
    ) -> Result<(), ControlError> {
        Ok(())
    }

    fn receive_computed_block(&mut self, _block_hash: H256) -> Result<(), ControlError> {
        Ok(())
    }

    fn is_block_producer(&self) -> Result<bool, anyhow::Error> {
        Ok(false)
    }
}

impl Stream for SimpleConnectService {
    type Item = ControlEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(event))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for SimpleConnectService {
    fn is_terminated(&self) -> bool {
        false
    }
}

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

struct Coordinator {}

struct Participant {}

struct ChainHeadReceived {
    block: SimpleBlockData,
    producer_blocks: Vec<SignedProducerBlock>,
}

pub struct Verifier {
    validators: Vec<Address>,
    producer: Address,
    block: SimpleBlockData,
    state: VerifierState,
}

enum VerifierState {
    WaitingParentComputed {
        parent_hash: H256,
    },
    WaitingForBlock,
    WaitingProducerBlockComputed {
        // +_+_+ think about
        block_hash: H256,
        parent_hash: Option<H256>,
    },
    WaitingForCommitment,
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
            let parent_hash = block.header.parent_hash;
            let producer_blocks = mem::take(&mut state.producer_blocks);

            if let Some(producer_block) = producer_blocks.into_iter().find(|block| {
                if block.block.block_hash != data.block_hash {
                    unreachable!("Guarantied by service impl: block hashes must be equal");
                }

                let Ok(pk) = block
                    .ecdsa_signature
                    .recover_from_digest(block.block.to_digest())
                else {
                    return false;
                };

                pk.to_address() == producer
            }) {
                self.state = State::Verifier(Verifier {
                    validators: data.validators,
                    producer,
                    block,
                    state: VerifierState::WaitingForBlock,
                });
                self.receive_block_from_producer(producer_block)?;
            } else {
                self.state = State::Verifier(Verifier {
                    validators: data.validators,
                    producer,
                    block,
                    state: VerifierState::WaitingParentComputed { parent_hash },
                });
                self.output
                    .push_back(ControlEvent::ComputeBlock(parent_hash));
            }
        }

        Ok(())
    }

    fn receive_block_from_producer(
        &mut self,
        signed: SignedProducerBlock,
    ) -> Result<(), ControlError> {
        match &mut self.state {
            State::Initial => {
                // TODO +_+_+: consider to collect producer blocks in this state.
                Err(ControlError::EventSkipped)
            }
            State::NewChainHeadReceived(state) => {
                // Wait for the block to be synced.
                if signed.block.block_hash == state.block.hash {
                    state.producer_blocks.push(signed);
                    Ok(())
                } else {
                    Err(ControlError::Warning(anyhow!(
                        "Received block {} is different from the expected block hash {}",
                        signed.block.block_hash,
                        state.block.hash
                    )))
                }
            }
            State::Verifier(verifier) => {
                // +_+_+ make a method recover for SignedProducerBlock
                let Ok(pk) = signed
                    .ecdsa_signature
                    .recover_from_digest(signed.block.to_digest())
                else {
                    log::warn!("Failed to recover public key from signature");
                    return Ok(());
                };

                if pk.to_address() != verifier.producer {
                    log::warn!("Received block from wrong producer");
                    return Ok(());
                }

                match verifier.state {
                    VerifierState::WaitingParentComputed { parent_hash } => {
                        verifier.state = VerifierState::WaitingProducerBlockComputed {
                            block_hash: signed.block.block_hash,
                            parent_hash: Some(parent_hash),
                        };
                        self.output
                            .push_back(ControlEvent::ComputeProducerBlock(signed.block));
                    }
                    VerifierState::WaitingForBlock => {
                        verifier.state = VerifierState::WaitingProducerBlockComputed {
                            block_hash: signed.block.block_hash,
                            parent_hash: None,
                        };
                        self.output
                            .push_back(ControlEvent::ComputeProducerBlock(signed.block));
                    }
                    _ => {
                        log::warn!("Received not waited block from producer");
                    }
                }

                Ok(())
            }
            State::Producer(_) => Err(ControlError::Warning(anyhow!(
                "Received producer block in producer mode"
            ))),
            State::Coordinator(coordinator) => todo!(),
            State::Participant(participant) => todo!(),
        }
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<(), ControlError> {
        match &mut self.state {
            State::Producer(producer) => {
                if let Some(signed_batch) = producer.receive_computed_block(computed_block)? {
                    self.output
                        .push_back(ControlEvent::RequestValidation(signed_batch));
                    // +_+_+ 
                    self.state = State::Coordinator(Coordinator {});
                } else {
                    // Empty block - nothing to do until next chain head.
                    self.state = State::Initial;
                }
            }
            State::Verifier(verifier) => match verifier.state {
                VerifierState::WaitingProducerBlockComputed {
                    block_hash,
                    parent_hash,
                } => {
                    if computed_block == block_hash {
                        verifier.state = VerifierState::WaitingForCommitment;
                        self.output.push_back(ControlEvent::SendValidationRequest);
                    } else if Some(computed_block) == parent_hash {
                        // Nothing
                    } else {
                        log::warn!(
                            "Received computed block {} is different from the expected block hash {}",
                            computed_block,
                            block_hash
                        );
                    }
                }
                VerifierState::WaitingParentComputed { parent_hash } => {
                    if parent_hash == computed_block {
                        verifier.state = VerifierState::WaitingForBlock;
                        self.output
                            .push_back(ControlEvent::ComputeBlock(parent_hash));
                    } else {
                        log::warn!(
                            "Received computed block {} is different from the expected parent block hash {}",
                            computed_block,
                            parent_hash
                        );
                    }
                }
                _ => {
                    log::warn!("Received computed block {computed_block} in unexpected state");
                }
            },
            _ => log::warn!("Received computed block {computed_block} in unexpected state"),
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
