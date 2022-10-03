use gear_core::ids::{CodeId, ProgramId};
use std::collections::BTreeSet;

use super::report::Report;

// TODO DN
#[derive(Default)]
pub struct ContextUpdate {
    program_ids: BTreeSet<ProgramId>,
}

#[derive(Clone, Default)]
pub struct Context {
    pub programs: BTreeSet<ProgramId>, // for send_message/send_reply
    pub codes: BTreeSet<CodeId>,
    // pub mailbox: Vec<Mailbox>, // for send_reply and claim_value
}

impl From<Report> for ContextUpdate {
    fn from(other: Report) -> Self {
        ContextUpdate {
            program_ids: other.program_ids,
        }
    }
}

impl Context {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, mut update: ContextUpdate) {
        self.programs.append(&mut update.program_ids)
    }
}
