use gear_core::ids::{CodeId, MessageId, ProgramId};
use std::collections::BTreeSet;

use super::report::{MailboxReport, Report};

#[derive(Debug, Default)]
pub struct ContextUpdate {
    program_ids: BTreeSet<ProgramId>,
    codes: BTreeSet<CodeId>,
    added_mailbox: BTreeSet<MessageId>,
    removed_mailbox: BTreeSet<MessageId>,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ProgramId>,
    pub codes: BTreeSet<CodeId>,
    pub mailbox_state: BTreeSet<MessageId>,
}

impl From<Report> for ContextUpdate {
    fn from(report: Report) -> Self {
        let Report {
            codes,
            program_ids,
            mailbox_data: MailboxReport { added, removed },
        } = report;
        ContextUpdate {
            program_ids,
            codes,
            added_mailbox: added,
            removed_mailbox: removed,
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
        self.mailbox_state
            .retain(|mid| !update.removed_mailbox.contains(mid));
        self.mailbox_state.append(&mut update.added_mailbox);
    }
}
