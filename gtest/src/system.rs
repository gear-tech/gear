use crate::{
    log::RunResult,
    manager::{Actor, ExtManager},
    program::{Program, ProgramIdWrapper},
};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::{
    message::Message,
    program::{CodeHash, ProgramId},
};
use path_clean::PathClean;
use std::{cell::RefCell, env, fs, io::Write, path::Path, thread};

pub struct System(pub(crate) RefCell<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(RefCell::new(ExtManager::new()))
    }
}

impl System {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init_logger(&self) {
        let _ = Builder::from_env(Env::default().default_filter_or("gwasm=debug"))
            .format(|buf, record| {
                let lvl = record.level().to_string().to_uppercase();
                let target = record.target().to_string();
                let mut msg = record.args().to_string();

                if target == "gwasm" {
                    msg = msg.replacen("DEBUG: ", "", 1);

                    writeln!(
                        buf,
                        "[{} {}] {}",
                        lvl.blue(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                } else {
                    writeln!(
                        buf,
                        "[{} {}] {}",
                        target.red(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                }
            })
            .format_target(false)
            .format_timestamp(None)
            .try_init();
    }

    pub fn send_message(&self, message: Message) -> RunResult {
        self.0.borrow_mut().run_message(message)
    }

    pub fn spend_blocks(&self, amount: u32) {
        self.0.borrow_mut().block_info.height += amount;
        self.0.borrow_mut().block_info.timestamp += amount as u64;
    }

    pub fn get_program<ID: Into<ProgramIdWrapper>>(&'_ self, id: ID) -> Option<Program<'_>> {
        let id = id.into().0;
        self.0
            .borrow()
            .actors
            .get(&id)
            .and_then(|(actor, _)| match actor {
                Actor::Dormant => None,
                _ => Some(Program {
                    id,
                    manager: &self.0,
                }),
            })
    }

    pub fn initialized_programs(&self) -> Vec<ProgramId> {
        self.0
            .borrow()
            .actors
            .iter()
            .filter_map(|(&program_id, (actor, _))| match actor {
                Actor::Initialized(_) => Some(program_id),
                _ => None,
            })
            .collect()
    }

    /// Saves code to the storage and returns it's code hash
    ///
    /// This method is mainly used for providing a proper program from program creation logic.
    /// In order to successfully create a new program with `gstd::prog::create_program_with_gas`
    /// function, developer should provide to the function "child's" code hash. Code for that
    /// code hash must be in storage at the time of the function call. So this method stores
    /// the code in storage.
    pub fn submit_code<P: AsRef<Path>>(&self, code_path: P) -> CodeHash {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));
        self.0.borrow_mut().store_new_code(&code)
    }
}
