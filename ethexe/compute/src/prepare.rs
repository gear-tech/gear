use crate::{utils, ComputeError, Result};
use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, OnChainStorageRead},
    events::{BlockEvent, RouterEvent},
    SimpleBlockData,
};
use gprimitives::{CodeId, H256};
use std::collections::{HashSet, VecDeque};

pub(crate) struct PrepareInfo {
    pub chain: VecDeque<SimpleBlockData>,
    pub missing_codes: HashSet<CodeId>,
    pub missing_validated_codes: HashSet<CodeId>,
}

pub(crate) fn prepare<
    DB: OnChainStorageRead + BlockMetaStorageRead + BlockMetaStorageWrite + CodesStorageRead,
>(
    db: &DB,
    head: H256,
) -> Result<PrepareInfo> {
    let chain = utils::collect_chain(db, head, |meta| !meta.prepared)?;

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();

    for block in chain.iter() {
        let events = db
            .block_events(block.hash)
            .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;
        let (block_missing_codes, block_missing_validated_codes) =
            propagate_data_from_parent(db, block.hash, block.header.parent_hash, events.iter())?;
        missing_codes.extend(block_missing_codes);
        missing_validated_codes.extend(block_missing_validated_codes);
    }

    Ok(PrepareInfo {
        chain,
        missing_codes,
        missing_validated_codes,
    })
}

/// # Return
/// (all missing codes, missing codes that have been already validated)
fn propagate_data_from_parent<
    'a,
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + CodesStorageRead,
>(
    db: &DB,
    block: H256,
    parent: H256,
    events: impl Iterator<Item = &'a BlockEvent>,
) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();
    let mut requested_codes = HashSet::new();
    let mut validated_codes = HashSet::new();
    let mut last_committed_batch = db
        .last_committed_batch(parent)
        .ok_or_else(|| ComputeError::LastCommittedBatchNotFound(parent))?;

    for event in events {
        match event {
            BlockEvent::Router(RouterEvent::BatchCommitted { digest }) => {
                last_committed_batch = *digest;
            }
            BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                requested_codes.insert(*code_id);
                if db.code_valid(*code_id).is_none() {
                    missing_codes.insert(*code_id);
                }
            }
            BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid }) => {
                validated_codes.insert(*code_id);
                match db.code_valid(*code_id) {
                    None => {
                        missing_validated_codes.insert(*code_id);
                        missing_codes.insert(*code_id);
                    }
                    Some(local_status) if local_status != *valid => {
                        return Err(ComputeError::CodeValidationStatusMismatch {
                            code_id: *code_id,
                            local_status,
                            remote_status: *valid,
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Propagate last committed batch
    db.set_last_committed_batch(block, last_committed_batch);

    // Propagate `wait for code validation` blocks queue
    let mut codes_queue = db
        .block_codes_queue(parent)
        .ok_or(ComputeError::CodesQueueNotFound(parent))?;
    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);
    db.set_block_codes_queue(block, codes_queue);

    Ok((missing_codes, missing_validated_codes))
}
