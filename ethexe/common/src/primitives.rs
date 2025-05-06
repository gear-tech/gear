use crate::events::BlockEvent;
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use gear_core::ids::ProgramId;
use gprimitives::{ActorId, MessageId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

impl BlockHeader {
    pub fn dummy(height: u32) -> Self {
        let mut parent_hash = [0; 32];
        parent_hash[..4].copy_from_slice(&height.to_le_bytes());

        Self {
            height,
            timestamp: height as u64 * 12,
            parent_hash: parent_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ProducerBlock {
    pub block_hash: H256,
    pub gas_allowance: Option<u64>,
    pub off_chain_transactions: Vec<H256>,
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ProgramId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ProgramId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ProgramId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

/// Scheduler; (block height, scheduled task)
pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;
