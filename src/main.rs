mod runner;

use anyhow::anyhow;
use gear_core::{
    memory::PageNumber,
    message::Message,
    program::{Program, ProgramId},
};
use test_gear_sample::sample::{self, Test};
use std::{fs, fmt};
use termion::{color, style};
use derive_more::Display;

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
pub struct ContentMismatch<T: std::fmt::Display+std::fmt::Debug> {
    expected: T,
    actual: T,
}

#[derive(Debug, Display)]
pub enum MessageContentMismatch {
    Destination(ContentMismatch<ProgramId>),
    Payload(ContentMismatch<DisplayedPayload>),
    GasLimit(ContentMismatch<u64>),
}

#[derive(Debug, Display)]
pub enum MessagesError {
    Count(ContentMismatch<usize>),
    #[display(fmt = "at position: {}, mismatch {}", at, mismatch)]
    AtPosition { at: usize, mismatch: MessageContentMismatch }
}

impl MessagesError {
    fn count(expected: usize, actual: usize) -> Self {
        Self::Count(ContentMismatch { expected, actual })
    }

    fn payload(at: usize, expected: Vec<u8>, actual: Vec<u8>) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Payload(
                ContentMismatch { expected: expected.into(), actual: actual.into() }
            ),
        }
    }

    fn destination(at: usize, expected: ProgramId, actual: ProgramId) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::Destination(
                ContentMismatch { expected: expected.into(), actual: actual.into() }
            ),
        }
    }

    fn gas_limit(at: usize, expected: u64, actual: u64) -> Self {
        Self::AtPosition {
            at,
            mismatch: MessageContentMismatch::GasLimit(
                ContentMismatch { expected: expected.into(), actual: actual.into() }
            ),
        }
    }
}

fn check_messages(
    messages: &[Message],
    expected_messages: &[sample::Message],
) -> Result<(), Vec<MessagesError>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push(MessagesError::count(expected_messages.len(), messages.len()))
    } else {
        expected_messages
            .iter()
            .zip(messages.iter())
            .enumerate()
            .for_each(|(position, (exp, msg))| {
                if ProgramId::from(exp.destination) != msg.dest {
                    errors.push(MessagesError::destination(position, exp.destination.into(), msg.dest))
                }
                if exp.payload.clone().into_raw() != msg.payload.clone().into_raw() {
                    errors.push(MessagesError::payload(position, exp.payload.clone().into_raw(), msg.payload.clone().into_raw()))
                }
                if let Some(expected_gas_limit) = exp.gas_limit {
                    if exp.gas_limit != msg.gas_limit {
                        errors.push(MessagesError::gas_limit(position, expected_gas_limit, msg.gas_limit.unwrap_or_default()))
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
    pages: &[(PageNumber, ProgramId)],
    expected_pages: &[sample::AllocationStorage],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if expected_pages.len() != pages.len() {
        errors.push("Expectation error (pages count doesn't match)\n".to_string());
    } else {
        expected_pages
            .iter()
            .zip(pages.iter())
            .for_each(|(exp, page)| {
                if exp.page_num != page.0.raw() {
                    errors.push(format!(
                        "Expectation error (PageNumber doesn't match, expected: {}, found: {})",
                        exp.page_num,
                        page.0.raw()
                    ));
                }
                if ProgramId::from(exp.program_id) != page.1 {
                    errors.push(format!(
                        "Expectation error (ProgramId doesn't match, expected: {}, found: {:?})\n",
                        exp.program_id, page.1
                    ));
                }
            });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_memory(
    persistent_memory: &[u8],
    program_storage: &mut Vec<Program>,
    expected_memory: &[sample::MemoryVariant],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        match case {
            sample::MemoryVariant::Static(case) => {
                if let Some(id) = case.program_id {
                    for p in &mut *program_storage {
                        if p.id() == ProgramId::from(id)
                            && p.static_pages()[case.address..case.address + case.bytes.len()]
                                != case.bytes
                        {
                            errors.push(
                                "Expectation error (Static memory doesn't match)".to_string(),
                            );
                        }
                    }
                }
            }
            sample::MemoryVariant::Shared(case) => {
                let offset = 256 * 65536;
                if persistent_memory
                    [case.address - offset..case.address - offset + case.bytes.len()]
                    != case.bytes
                {
                    errors.push("Expectation error (Shared memory doesn't match)".to_string());
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
    let u = serde_json::from_reader(file)
        .map_err(|e| anyhow::anyhow!("Error decoding '{}': {}", path.as_ref().display(), e))?;

    Ok(u)
}

pub fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut tests = Vec::new();

    for f in std::env::args().skip(1) {
        if fs::metadata(&f).map(|m| m.is_dir()).unwrap_or_else(|e| {
            println!("Error accessing {}: {}", f, e);
            false
        }) {
            continue;
        }

        tests.push(read_test_from_file(&f)?);
    }

    let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
    let mut total_failed = 0i32;

    println!("Total fixtures: {}", total_fixtures);

    for test in tests {
        for fixture_no in 0..test.fixtures.len() {
            for exp in &test.fixtures[fixture_no].expected {
                let output = match runner::init_fixture(&test, fixture_no) {
                    Ok(initialized_fixture) => match runner::run(initialized_fixture, exp.step) {
                        Ok((mut final_state, persistent_memory)) => {
                            let mut errors = Vec::new();
                            if let Some(messages) = &exp.messages {
                                if let Err(msg_errors) = check_messages(&final_state.messages, messages) {
                                    errors.extend(
                                        msg_errors.into_iter().map(|err| format!("Messages check [{}]", err))
                                    );
                                }
                            }
                            if let Some(log) = &exp.log {
                                if let Err(log_errors) = check_messages(&final_state.log, log) {
                                    errors.extend(
                                        log_errors.into_iter().map(|err| format!("Log check [{}]", err))
                                    );
                                }
                            }
                            if let Some(alloc) = &exp.allocations {
                                if let Err(alloc_errors) =
                                    check_allocations(&final_state.allocation_storage, alloc)
                                {
                                    errors.extend(alloc_errors);
                                }
                            }
                            if let Some(mem) = &exp.memory {
                                if let Err(mem_errors) = check_memory(
                                    &persistent_memory,
                                    &mut final_state.program_storage,
                                    mem,
                                ) {
                                    errors.extend(mem_errors);
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
                                "{}Running error ({}){}",
                                color::Fg(color::Red),
                                e,
                                style::Reset
                            )
                        }
                    },
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
