use crate::runner::{self, CollectState};
use anyhow::anyhow;
use derive_more::Display;
use gear_core::{
    memory::PAGE_SIZE,
    message::Message,
    program::{Program, ProgramId},
    storage,
};
use crate::sample::{self, Test};
use std::{fmt, fs};
use termion::{color, style};

#[derive(Debug, derive_more::From)]
pub struct DisplayedPayload(Vec<u8>);

impl fmt::Display for DisplayedPayload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(utf8) = std::str::from_utf8(&self.0[..]) {
            write!(f, "utf-8 ({})", utf8)
        } else {
            write!(f, "bytes ({:?})", &self.0[..])
        }
    }
}

#[derive(Debug, Display)]
#[display(fmt = "expected: {}, actual: {}", expected, actual)]
pub struct ContentMismatch<T: std::fmt::Display + std::fmt::Debug> {
    expected: T,
    actual: T,
}

#[derive(Debug, Display)]
pub enum MessageContentMismatch {
    Destination(ContentMismatch<ProgramId>),
    Payload(ContentMismatch<DisplayedPayload>),
    GasLimit(ContentMismatch<u64>),
    ExitCode(ContentMismatch<i32>),
}

#[derive(Debug, Display)]
pub enum MessagesError {
    Count(ContentMismatch<usize>),
    #[display(fmt = "at position: {}, mismatch {}", at, mismatch)]
    AtPosition {
        at: usize,
        mismatch: MessageContentMismatch,
    },
}

impl MessagesError {
    fn count(expected: usize, actual: usize) -> Self {
        Self::Count(ContentMismatch { expected, actual })
    }

    fn payload(at: usize, expected: Vec<u8>, actual: Vec<u8>) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Payload(ContentMismatch {
                expected: expected.into(),
                actual: actual.into(),
            }),
        }
    }

    fn destination(at: usize, expected: ProgramId, actual: ProgramId) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Destination(ContentMismatch { expected, actual }),
        }
    }

    fn gas_limit(at: usize, expected: u64, actual: u64) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::GasLimit(ContentMismatch { expected, actual }),
        }
    }

    fn exit_code(at: usize, expected: i32, actual: i32) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::ExitCode(ContentMismatch { expected, actual }),
        }
    }
}

fn check_messages(
    messages: &[Message],
    expected_messages: &[sample::Message],
) -> Result<(), Vec<MessagesError>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push(MessagesError::count(
            expected_messages.len(),
            messages.len(),
        ))
    } else {
        expected_messages
            .iter()
            .zip(messages.iter())
            .enumerate()
            .for_each(|(position, (exp, msg))| {
                if ProgramId::from(exp.destination) != msg.dest {
                    errors.push(MessagesError::destination(
                        position,
                        exp.destination.into(),
                        msg.dest,
                    ))
                }
                if exp
                    .payload
                    .as_ref()
                    .map(|payload| !payload.equals(msg.payload.as_ref()))
                    .unwrap_or(false)
                {
                    errors.push(MessagesError::payload(
                        position,
                        exp.payload
                            .as_ref()
                            .expect("Checked above.")
                            .clone()
                            .into_raw(),
                        msg.payload.clone().into_raw(),
                    ))
                }
                if let Some(expected_gas_limit) = exp.gas_limit {
                    if expected_gas_limit != msg.gas_limit {
                        errors.push(MessagesError::gas_limit(
                            position,
                            expected_gas_limit,
                            msg.gas_limit,
                        ))
                    }
                }

                if let Some(expected_exit_code) = exp.exit_code {
                    match msg.reply {
                        Some((_, exit_code)) => {
                            if exit_code != expected_exit_code {
                                errors.push(MessagesError::exit_code(
                                    position,
                                    expected_exit_code,
                                    exit_code,
                                ))
                            }
                        }
                        None => {
                            if expected_exit_code != 0 {
                                errors.push(MessagesError::exit_code(
                                    position,
                                    expected_exit_code,
                                    0,
                                ))
                            }
                        }
                    }
                }
            });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_allocations(
    programs: &[Program],
    expected_pages: &[sample::AllocationStorage],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for exp in expected_pages {
        for program in programs {
            if ProgramId::from(exp.program_id) == program.id() {
                if !program.get_pages().contains_key(&exp.page_num.into()) {
                    errors.push(format!(
                        "Expectation error (PageNumber doesn't match, expected: {})",
                        exp.page_num
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_memory(
    program_storage: &mut Vec<Program>,
    expected_memory: &[sample::BytesAt],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        for p in &mut *program_storage {
            if p.id() == ProgramId::from(case.program_id) {
                let page = case.address / PAGE_SIZE;
                if let Some(page_buf) = p.get_page((page as u32).into()) {
                    if page_buf[case.address - page * PAGE_SIZE
                        ..(case.address - page * PAGE_SIZE) + case.bytes.len()]
                        != case.bytes
                    {
                        errors.push("Expectation error (Static memory doesn't match)".to_string());
                    }
                } else {
                    errors.push("Expectation error (Incorrect memory address)".to_string());
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn read_test_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Test> {
    let file = fs::File::open(path.as_ref())
        .map_err(|e| anyhow::anyhow!("Error loading '{}': {}", path.as_ref().display(), e))?;
    let u = serde_yaml::from_reader(file)
        .map_err(|e| anyhow::anyhow!("Error decoding '{}': {}", path.as_ref().display(), e))?;

    Ok(u)
}

pub fn check_main<MQ: storage::MessageQueue, PS: storage::ProgramStorage>(
    files: Vec<std::path::PathBuf>,
    skip_messages: bool,
    skip_allocations: bool,
    skip_memory: bool,
    print_log: bool,
    storage_factory: impl Fn() -> storage::Storage<MQ, PS>,
) -> anyhow::Result<()>
where
    storage::Storage<MQ, PS>: CollectState,
{
    let mut tests = Vec::new();

    for path in files {
        if path.is_dir() {
            for entry in path.read_dir().expect("read_dir call failed").flatten() {
                tests.push(read_test_from_file(&entry.path())?);
            }
        } else {
            tests.push(read_test_from_file(&path)?);
        }
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let mut total_failed = 0i32;

    println!("Total fixtures: {}", total_fixtures);

    for test in tests {
        for fixture_no in 0..test.fixtures.len() {
            for exp in &test.fixtures[fixture_no].expected {
                let output = match runner::init_fixture(storage_factory(), &test, fixture_no) {
                    Ok(initialized_fixture) => {
                        let (mut final_state, _result) = runner::run(initialized_fixture, exp.step);

                        let mut errors = Vec::new();
                        if !skip_messages {
                            if let Some(messages) = &exp.messages {
                                if let Err(msg_errors) =
                                    check_messages(&final_state.messages, messages)
                                {
                                    errors.extend(
                                        msg_errors
                                            .into_iter()
                                            .map(|err| format!("Messages check [{}]", err)),
                                    );
                                }
                            }
                        }
                        if let Some(log) = &exp.log {
                            if print_log {
                                for message in &final_state.log {
                                    if let Ok(utf8) = std::str::from_utf8(message.payload()) {
                                        println!("log({})", utf8)
                                    }
                                }
                            }
                            if let Err(log_errors) = check_messages(&final_state.log, log) {
                                errors.extend(
                                    log_errors
                                        .into_iter()
                                        .map(|err| format!("Log check [{}]", err)),
                                );
                            }
                        }
                        if !skip_allocations {
                            if let Some(alloc) = &exp.allocations {
                                if let Err(alloc_errors) =
                                    check_allocations(&final_state.program_storage, alloc)
                                {
                                    errors.extend(alloc_errors);
                                }
                            }
                        }
                        if !skip_memory {
                            if let Some(mem) = &exp.memory {
                                if let Err(mem_errors) =
                                    check_memory(&mut final_state.program_storage, mem)
                                {
                                    errors.extend(mem_errors);
                                }
                            }
                        }

                        if !errors.is_empty() {
                            total_failed += 1;
                            errors.insert(0, format!("{}", color::Fg(color::Red)));
                            errors.insert(errors.len(), format!("{}", style::Reset));
                            errors.join("\n")
                        } else {
                            format!("{}Ok{}", color::Fg(color::Green), style::Reset)
                        }
                    }
                    Err(e) => {
                        total_failed += 1;
                        format!(
                            "{}Initialization error ({}){}",
                            color::Fg(color::Red),
                            e,
                            style::Reset
                        )
                    }
                };

                println!(
                    "Fixture {}{}{}: {}",
                    style::Bold,
                    test.fixtures[fixture_no].title,
                    style::Reset,
                    output
                );
            }
        }
    }

    if total_failed == 0 {
        Ok(())
    } else {
        Err(anyhow!("{} tests failed", total_failed))
    }
}
