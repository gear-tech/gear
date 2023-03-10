use gear_core::ids::{CodeId, MessageId, ProgramId};
use std::collections::BTreeSet;

use super::report::{ExtrinsicReport, StateReport};

#[derive(Default)]
pub struct ContextUpdate {
    program_ids: BTreeSet<ProgramId>,
    codes: BTreeSet<CodeId>,
    mailbox_state: BTreeSet<(MessageId, u128)>,
}

#[derive(Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ProgramId>,
    pub codes: BTreeSet<CodeId>,
    pub mailbox_state: BTreeSet<(MessageId, u128)>,
}

impl From<(ExtrinsicReport, StateReport)> for ContextUpdate {
    fn from(reports: (ExtrinsicReport, StateReport)) -> Self {
        let ExtrinsicReport {
            codes,
            program_ids,
        } = reports.0;
        let StateReport { current_mailbox } = reports.1;
        ContextUpdate {
            program_ids,
            codes,
            mailbox_state: current_mailbox,
        }
    }
}

impl Context {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, mut update: ContextUpdate) {
        self.programs.append(&mut update.program_ids);
        self.codes.append(&mut update.codes);
        self.mailbox_state.append(&mut update.mailbox_state);
    }
}
