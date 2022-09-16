//! Reporter for each task used as an in-memory
//! logger to solve logs jumbling problem in a
//! multithreaded environment.

// todo use cow?
// todo make macro to use "{}" stuff instead of data argument in `record`
#[derive(Debug)]
pub(crate) struct Reporter {
    seed: u64,
    reports: Vec<String>
}

impl Reporter {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            seed: id,
            reports: Vec::new(),
        }
    }

    pub(crate) fn record(&mut self, data: impl ToString) {
        self.reports.push(data.to_string());
    }

    pub(crate) fn report(self) {
        println!("Reporter with seed {} reports:", self.seed);
        println!("==============================================");
        self.reports.into_iter().for_each(|record| {
            println!("{record:?}");
        });
        println!("==============================================\n");
    }
}


