//! Reporter for each task used as an in-memory
//! logger to solve logs jumbling problem in a
//! multithreaded environment.

use anyhow::Result;

pub(crate) type SomeReporter = Box<dyn Reporter>;

pub(crate) trait Reporter: Send {
    /// Save data to reported later.
    fn record(&mut self, data: String) -> Result<()>;

    /// Report saved data into any destination either file, socket
    /// or stdout.
    fn report(&self) -> Result<()>;
}

// todo make macro to use "{}" stuff instead of data argument in `record`
#[derive(Debug)]
pub(crate) struct StdoutReporter {
    id: u64,
    reports: Vec<String>,
}

impl StdoutReporter {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            id,
            reports: Vec::new(),
        }
    }
}

impl Reporter for StdoutReporter {
    fn record(&mut self, data: String) -> Result<()> {
        self.reports.push(data);
        Ok(())
    }

    fn report(&self) -> Result<()> {
        println!("Reporter with seed {} reports:", self.id);
        println!("==============================================");
        self.reports.iter().for_each(|record| {
            println!("{record:?}");
        });
        println!("==============================================\n");
        Ok(())
    }
}
