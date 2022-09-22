use super::context::TaskContextUpdate;

// Todo DN maybe queue for guaranteeing the order?
pub(super) type PreRunReport = Vec<String>;
pub(super) type PostRunReport = Vec<String>;

pub(super) struct BatchRunReport {
    pub(super) seed: u64,
    // Todo DN maybe queue for guaranteeing the order?
    pub(super) reports: Vec<String>,
    pub(super) context_update: TaskContextUpdate,
}

impl BatchRunReport {
    pub(super) fn new(_seed: u64, _: PreRunReport, _: PostRunReport, _: TaskContextUpdate) -> Self {
        // Order of tasks in pre-run_reports and post_run_reports is the same.
        todo!("Todo DN")
    }
}

pub(super) trait TaskReporter {
    fn report(&self) -> String;
}
