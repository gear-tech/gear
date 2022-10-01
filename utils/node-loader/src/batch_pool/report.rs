use super::context::ContextUpdate;
use gclient::Error;
use gear_core::ids::{ProgramId, CodeId};
use std::collections::BTreeSet;

// Todo DN maybe queue for guaranteeing the order?
pub type PreRunReport = Vec<String>;

pub struct Report {
    pub logs: Vec<String>,
    // todo Option
    pub codes: BTreeSet<CodeId>,
    // todo Option
    pub program_ids: BTreeSet<ProgramId>,
    pub blocks_stopped: bool,
}

#[derive(Default)]
pub struct BatchRunReport {
    // Todo DN maybe queue for guaranteeing the order?
    pub reports: Vec<String>,
    pub context_update: ContextUpdate,
    pub blocks_stopped: bool,
}

impl BatchRunReport {
    pub fn new(mut pre: PreRunReport, mut report: Report) -> Self {
        let mut reports = vec![];

        reports.append(pre.as_mut());
        reports.push(String::from("RESULTS:"));
        reports.append(report.logs.as_mut());

        Self {
            reports,
            blocks_stopped: report.blocks_stopped,
            context_update: report.into(),
        }
    }

    pub fn from_err(mut pre: PreRunReport, err: Error) -> Self {
        let mut reports = vec![];

        reports.append(pre.as_mut());
        reports.push(String::from("ERROR:"));
        reports.push(err.to_string());

        Self {
            reports,
            ..Default::default()
        }
    }
}

pub trait BatchReporter {
    fn report(&self) -> Vec<String>;
}
