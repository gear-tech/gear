//! Log filter for the node
use anyhow::{anyhow, Result};
use smallvec::SmallVec;
use std::{
    io::{BufRead, BufReader},
    process::Child,
    thread,
    thread::JoinHandle,
};

/// Log filter for the node
#[derive(Default)]
pub struct Log {
    /// Join handle of logs
    handle: Option<JoinHandle<()>>,
    /// Filter of the stored logs
    ///
    /// i.e. this filter will match the start pattern
    /// of the output log of our node
    pub filter: Vec<String>,
    /// Filtered logs from the node output
    pub logs: SmallVec<[String; 256]>,
}

impl Log {
    /// Create log holder from child process
    pub fn spawn(&mut self, ps: &mut Child) -> Result<()> {
        let Some(stderr) = ps.stderr.take() else {
            return Err(anyhow!("Not stderr found"));
        };

        let handle = thread::spawn(move || {
            for line in BufReader::new(stderr)
                .lines()
                .map_while(|result| result.ok())
            {
                println!("{}", line);
            }
        });

        self.handle = Some(handle);
        Ok(())
    }
}
