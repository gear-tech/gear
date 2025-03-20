// +_+_+ doc

use std::{
    collections::VecDeque,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{anyhow, Result};
use ethexe_common::SimpleBlockData;
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{
    sha3::{digest::Update, Keccak256},
    Address, Digest, PublicKey, Signature, Signer, ToDigest,
};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use parity_scale_codec::Encode;

pub trait ControlService: Stream<Item = Result<ControlEvent>> + FusedStream {
    fn receive_new_chain_head(&mut self, block: SimpleBlockData);
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()>;
    fn receive_block_from_producer(&mut self, block: SignedProducerBlock) -> Result<()>;
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<()>;
    fn is_block_producer(&self) -> Result<bool>;
}

#[derive(Clone, Debug)]
pub struct ProducerBlock {
    block_hash: H256,
    gas_allowance: Option<u64>,
    // +_+_+ consider. Maybe need to share off-chain transactions data.
    off_chain_transactions: Vec<H256>,
}

impl ToDigest for ProducerBlock {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        hasher.update(self.block_hash.as_bytes());
        hasher.update(self.gas_allowance.encode().as_slice());
        hasher.update(self.off_chain_transactions.encode().as_slice());
    }
}

pub enum ControlEvent {
    ComputeBlock(H256),
    ComputeProducerBlock(ProducerBlock),
    PublishProducerBlock(SignedProducerBlock),
    SendValidationRequest,
}

pub struct SignedProducerBlock {
    block: ProducerBlock,
    ecdsa_signature: Signature,
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

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        let Some(block) = self.block.as_ref() else {
            log::warn!(
                "Received synced block {}, but no chain-head was received yet",
                data.block_hash
            );
            return Ok(());
        };

        if block.hash != data.block_hash {
            log::warn!(
                "Received synced block {} is different from the expected block hash {}",
                data.block_hash,
                block.hash
            );
            return Ok(());
        }

        self.output
            .push_back(ControlEvent::ComputeBlock(block.header.parent_hash));

        Ok(())
    }

    fn receive_block_from_producer(&mut self, _block_hash: SignedProducerBlock) -> Result<()> {
        Ok(())
    }

    fn receive_computed_block(&mut self, _block_hash: H256) -> Result<()> {
        Ok(())
    }

    fn is_block_producer(&self) -> Result<bool> {
        Ok(false)
    }
}

impl Stream for SimpleConnectService {
    type Item = Result<ControlEvent>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(Ok(event)))
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

pub struct Producer {
    validators: Vec<Address>,
    block: SimpleBlockData,
    state: ProducerState,
}

pub struct Verifier {
    validators: Vec<Address>,
    producer: Address,
    block: SimpleBlockData,
    state: VerifierState,
}

enum ProducerState {
    CollectOffChainTransactions,
    WaitingBlockComputed(H256),
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
    pub fn new(pub_key: PublicKey, signer: Signer, slot_duration: u64) -> Self {
        Self {
            slot_duration,
            pub_key,
            signer,
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
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        let State::NewChainHeadReceived(state) = &mut self.state else {
            log::warn!("Received unexpected synced block {}", data.block_hash);
            return Ok(());
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
            let block_hash = block.hash;

            self.state = State::Producer(Producer {
                validators: data.validators,
                block,
                // TODO +_+_+: collect off-chain transactions is skipped for now
                state: ProducerState::WaitingBlockComputed(block_hash),
            });

            let block = ProducerBlock {
                block_hash,
                // +_+_+ set gas allowance here
                gas_allowance: Some(3_000_000_000_000),
                // +_+_+ append off-chain transactions
                off_chain_transactions: Vec::new(),
            };

            let ecdsa_signature = self.signer.sign_digest(self.pub_key, block.to_digest())?;

            self.output
                .push_back(ControlEvent::ComputeProducerBlock(block.clone()));
            self.output
                .push_back(ControlEvent::PublishProducerBlock(SignedProducerBlock {
                    block,
                    ecdsa_signature,
                }));
        } else {
            let parent_hash = block.header.parent_hash;
            let producer_blocks = mem::take(&mut state.producer_blocks);

            if let Some(producer_block) = producer_blocks.into_iter().find(|block| {
                if block.block.block_hash != data.block_hash {
                    return false;
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

    fn receive_block_from_producer(&mut self, signed: SignedProducerBlock) -> Result<()> {
        match &mut self.state {
            State::Initial => log::warn!("Received block from producer in initial state"),
            State::NewChainHeadReceived(state) => {
                // Wait for the block to be synced.
                state.producer_blocks.push(signed);
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
            }
            State::Producer(_) => {
                log::warn!("Receive block from producer in producer mode");
            }
            State::Coordinator(coordinator) => todo!(),
            State::Participant(participant) => todo!(),
        }

        Ok(())
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<()> {
        match &mut self.state {
            State::Producer(producer) => match producer.state {
                ProducerState::CollectOffChainTransactions => todo!(),
                ProducerState::WaitingBlockComputed(block_hash) => {
                    if computed_block != block_hash {
                        log::warn!(
                            "Received computed block {} is different from the expected block hash {}",
                            computed_block,
                            block_hash
                        );
                        return Ok(());
                    }

                    // +_+_+ generate batch commitment
                },
            },
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

    fn is_block_producer(&self) -> Result<bool> {
        // +_+_+
        todo!()
    }
}

impl Stream for ValidatorService {
    type Item = Result<ControlEvent>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.output.pop_front() {
            Poll::Ready(Some(Ok(event)))
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
