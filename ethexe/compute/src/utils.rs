use crate::{ComputeError, Result};
use ethexe_common::{
    db::{BlockMetaStorageRead, OnChainStorageRead},
    BlockMeta, SimpleBlockData,
};
use gprimitives::H256;
use std::collections::VecDeque;

/// Collect a chain of blocks from the head to the last block that satisfies the filter.
/// Stops when the filter returns false for the block meta.
/// Returns a chain sorted in order from the oldest to the newest block (head is newest).
pub fn collect_chain<DB: BlockMetaStorageRead + OnChainStorageRead>(
    db: &DB,
    head: H256,
    mut filter: impl FnMut(&BlockMeta) -> bool,
) -> Result<VecDeque<SimpleBlockData>> {
    let mut block = head;
    let mut chain = VecDeque::new();

    while filter(&db.block_meta(block)) {
        let header = db
            .block_header(block)
            .ok_or(ComputeError::BlockHeaderNotFound(block))?;

        let parent = header.parent_hash;

        chain.push_front(SimpleBlockData {
            hash: block,
            header,
        });

        block = parent;
    }

    Ok(chain)
}
