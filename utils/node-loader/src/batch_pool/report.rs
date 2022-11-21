use super::context::ContextUpdate;
use crate::utils;
use anyhow::Error;
use gear_core::ids::{CodeId, ProgramId};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CrashAlert {
    #[error("Crash alert: Message processing has been stopped")]
    MsgProcessingStopped,
    #[error("Crash alert: Timeout occurred while processing batch")]
    Timeout,
    #[error("Crash alert: Can't reach the node, considered to be dead")]
    NodeIsDead,
}

impl TryFrom<Error> for CrashAlert {
    type Error = Error;

    fn try_from(err: Error) -> Result<Self, Self::Error> {
        let err_string = err.to_string().to_lowercase();
        if err_string.contains(&utils::EVENTS_TIMEOUT_ERR_STR.to_lowercase()) {
            Ok(CrashAlert::Timeout)
        } else if err_string.contains(&utils::SUBXT_RPC_REQUEST_ERR_STR.to_lowercase()) {
            Ok(CrashAlert::NodeIsDead)
        } else {
            Err(err)
        }
    }
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
