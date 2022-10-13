use super::context::ContextUpdate;
use anyhow::Error;
use gear_core::ids::{CodeId, ProgramId};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Report {
    pub codes: BTreeSet<CodeId>,
    pub program_ids: BTreeSet<ProgramId>,
    pub blocks_stopped: bool,
}

impl Report {
    pub fn blocks_stopped() -> Self {
        Self {
            blocks_stopped: true,
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct BatchRunReport {
    /// Seed of the batch is the id.
    pub id: u64,
    pub context_update: ContextUpdate,
    pub blocks_stopped: bool,
    pub err: Option<Error>,
}

impl BatchRunReport {
    pub fn new(id: u64, report: Report) -> Self {
        Self {
            id,
            blocks_stopped: report.blocks_stopped,
            context_update: report.into(),
            err: None,
        }
    }

    pub fn from_err(err: Error) -> Self {
        Self {
            err: Some(err),
            ..Default::default()
        }
    }
}
