use super::runner::{Config, ExtMessage, InitializeProgramInfo, Runner};
use alloc::vec::*;
use gear_core::storage::{
    InMemoryMessageQueue, InMemoryProgramStorage, InMemoryStorage, InMemoryWaitList,
};

/// Builder for [`Runner`].
pub struct RunnerBuilder {
    config: Config,
    programs: Vec<InitializeProgramInfo>,
    storage: InMemoryStorage,
}

impl RunnerBuilder {
    /// Default [`Runner`].
    pub fn new() -> RunnerBuilder {
        RunnerBuilder {
            config: Config::default(),
            programs: vec![],
            storage: InMemoryStorage::default(),
        }
    }

    /// Set [`Config`].
    pub fn config(self, config: Config) -> RunnerBuilder {
        RunnerBuilder {
            config: config,
            programs: self.programs,
            storage: self.storage,
        }
    }

    /// Set [`Program`] to be initialized on build.
    pub fn program(
        mut self,
        source_id: u64,
        new_program_id: u64,
        code: Vec<u8>,
        message: ExtMessage,
    ) -> RunnerBuilder {
        self.programs.push(InitializeProgramInfo {
            source_id: source_id.into(),
            new_program_id: new_program_id.into(),
            message,
            code,
        });
        RunnerBuilder {
            config: self.config,
            programs: self.programs,
            storage: self.storage,
        }
    }

    /// Initialize all programs and return [`Runner`].
    pub fn build(self) -> Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList> {
        let mut runner = Runner::new(&self.config, self.storage);
        for program in self.programs {
            runner
                .init_program(program)
                .expect("failed to init program");
        }
        runner
    }
}
