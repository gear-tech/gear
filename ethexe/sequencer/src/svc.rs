#![allow(unused)]

use crate::{CodeCommitment, CommitmentsMap, MultisignedCommitmentDigests, SequencerStatus};
use anyhow::Result;
use ethexe_common::gear::BlockCommitment;
use ethexe_ethereum::Ethereum;
use ethexe_processor::Processor;
use ethexe_signer::{Address, PublicKey};
use gprimitives::{CodeId, H256};
use std::collections::{BTreeSet, HashSet};
use tokio::sync::mpsc;

pub struct Service {
    key: PublicKey,
    ethereum: Ethereum,
    validators: HashSet<Address>,
    threshold: u64,

    status: SequencerStatus,

    // state of the service. to be moved into separate struct?
    code_commitments: CommitmentsMap<CodeCommitment>,
    block_commitments: CommitmentsMap<BlockCommitment>,

    codes_candidate: Option<MultisignedCommitmentDigests>,
    blocks_candidate: Option<MultisignedCommitmentDigests>,
    chain_head: Option<H256>,
    waiting_for_commitments: BTreeSet<H256>,
}

impl Service {
    // creates production version of self
    pub fn new(config: &Config) -> Result<Self> {
        todo!("PR");
    }

    /// spawns `|| loop { select! { .. } }` in separate tokio thread
    /// returns external IO channels
    pub fn run(
        self,
    ) -> (
        mpsc::UnboundedSender<Request>,
        mpsc::UnboundedReceiver<Event>,
    ) {
        todo!("PR");
    }
}

pub struct Config {/* configuration */}

pub enum Event {
    TxSent, // ?
}

pub enum Request {
    HandleBlock(/* block data */), // after authorship handling
    HandleCode(CodeId),
    SyncStatus,
}
