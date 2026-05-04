use super::report::{MailboxReport, Report};
use gear_core::ids::{ActorId, CodeId, MessageId};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct ContextUpdate {
    program_ids: BTreeSet<ActorId>,
    codes: BTreeSet<CodeId>,
    added_mailbox: BTreeSet<MessageId>,
    removed_mailbox: BTreeSet<MessageId>,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ActorId>,
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
