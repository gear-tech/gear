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
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            programs: vec![],
            storage: InMemoryStorage::default(),
        }
    }

    /// Set [`Config`].
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Set [`Program`] to be initialized on build.
    pub fn program(self, code: Vec<u8>) -> ProgramBuilder {
        ProgramBuilder::new(self, code)
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

pub struct ProgramBuilder {
    runner_builder: RunnerBuilder,
    code: Vec<u8>,
    source_id: u64,
    new_program_id: u64,
    message: ExtMessage,
}

impl ProgramBuilder {
    pub fn new(runner_builder: RunnerBuilder, code: Vec<u8>) -> Self {
        let counter = runner_builder.programs.len() as u64;
        ProgramBuilder {
            runner_builder,
            code,
            source_id: 1001,
            new_program_id: 1 + counter,
            message: ExtMessage {
                id: (1000000 + counter).into(),
                payload: vec![],
                gas_limit: u64::MAX,
                value: 0,
            },
        }
    }

    pub fn build(self) -> RunnerBuilder {
        let mut runner = self.runner_builder;

        runner.programs.push(InitializeProgramInfo {
            source_id: self.source_id.into(),
            new_program_id: self.new_program_id.into(),
            code: self.code,
            message: self.message,
        });
        runner
    }

    pub fn source(mut self, id: u64) -> Self {
        self.source_id = id;
        self
    }

    pub fn id(mut self, id: u64) -> Self {
        self.new_program_id = id;
        self
    }

    pub fn init_message(mut self, message: ExtMessage) -> Self {
        self.message = message;
        self
    }
}
