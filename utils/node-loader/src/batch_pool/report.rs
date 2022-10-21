use super::context::ContextUpdate;
use gear_core::ids::{CodeId, ProgramId};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CrashAlert {
    #[error("Message processing has been stopped")]
    MsgProcessingStopped,
    #[error("Timeout occurred while processing batch")]
    Timeout,
    #[error("Can't reach the node, considered to be dead")]
    NodeIsDead,
}

#[derive(Default)]
pub struct Report {
    pub codes: BTreeSet<CodeId>,
    pub program_ids: BTreeSet<ProgramId>,
}

#[derive(Default)]
pub struct BatchRunReport {
    /// Seed of the batch is the id.
    pub id: u64,
    pub context_update: ContextUpdate,
}

impl BatchRunReport {
    pub fn new(id: u64, report: Report) -> Self {
        Self {
            id,
            context_update: report.into(),
        }
    }

    pub fn empty(id: u64) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
}
