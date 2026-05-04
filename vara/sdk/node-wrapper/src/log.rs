//! Log filter for the node
use anyhow::{Result, anyhow};
use smallvec::SmallVec;
use std::{
    io::{BufRead, BufReader, Read},
    process::Child,
    sync::{Arc, RwLock},
    thread,
    thread::JoinHandle,
};

const DEFAULT_LOGS_LIMIT: usize = 256;
const BLOCK_INITIALIZATION: &str = "Imported #1";

/// Log filter for the node
#[derive(Default)]
pub struct Log {
    /// Join handle of logs
    handle: Option<JoinHandle<()>>,
    /// Filtered logs from the node output
    pub logs: Arc<RwLock<SmallVec<[String; DEFAULT_LOGS_LIMIT]>>>,
}

impl Log {
    /// New log with holding limits
    pub fn new(limit: Option<usize>) -> Self {
        let mut this = Log::default();
        if let Some(limit) = limit {
            this.resize(limit);
        }

        this
    }

    /// Resize the limit logs that this instance holds
    pub fn resize(&mut self, limit: usize) {
        if let Ok(mut logs) = self.logs.write() {
            if limit > DEFAULT_LOGS_LIMIT {
                logs.grow(limit)
            } else {
                logs.reserve(limit)
            }
        }
    }

    /// Spawn logs from the child process
    pub fn spawn(&mut self, ps: &mut Child) -> Result<()> {
        let Some(stderr) = ps.stderr.take() else {
            return Err(anyhow!("Not stderr found"));
        };

        // Blocking after initialization.
        let mut reader = BufReader::new(stderr);
        for line in reader.by_ref().lines().map_while(|result| result.ok()) {
            if line.contains(BLOCK_INITIALIZATION) {
                break;
            }
        }

        // Mapping logs to memory
        let logs = Arc::clone(&self.logs);
        let handle = thread::spawn(move || {
            for line in reader.lines().map_while(|result| result.ok()) {
                if let Ok(mut logs) = logs.write() {
                    logs.push(line);
                }
            }
        });

        self.handle = Some(handle);
        Ok(())
    }
}
