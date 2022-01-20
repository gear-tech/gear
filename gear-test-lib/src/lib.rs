use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Mutex,
};
use anyhow::{anyhow, Result};

use gear_core::{
    program::{Program, ProgramId},
    message::{Message, MessageId},
};
use core_processor::configs::{AllocationsConfig, BlockInfo};

pub mod mock;

#[derive(Default)]
pub struct TestSystem(Mutex<System>);

#[derive(Default)]
struct System {
    block_info: BlockInfo,
    config: AllocationsConfig,
    nonce: u64,

    programs: BTreeMap<ProgramId, Program>,
    message_queue: VecDeque<Message>,
    mailbox: BTreeMap<ProgramId, Vec<Message>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,

    log: BTreeSet<Message>,
}
